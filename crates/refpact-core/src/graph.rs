//! Graphs of units or of slices: what the checks about cycles and chains share (SPEC 4.5 to
//! 4.9). A check gets a graph from [`crate::Scope::subject_graph`] or [`crate::Scope::unit_graph`].

use std::borrow::Cow;
use std::collections::VecDeque;

use crate::index::FastSet;
use crate::model::{Fact, FactSet, What};
use crate::pattern::{Path, Pattern};
use crate::violation::Step;

/// An edge of a graph of nodes, with the first reference that makes it.
#[derive(Debug, Clone, Copy)]
pub struct Edge<'a> {
    /// The node that the edge leads to.
    pub to: usize,
    /// The position of the reference in the facts.
    pub fact: usize,
    /// The path of the reference that leads to the node.
    pub target: &'a Path,
}

/// Valid slice patterns, from [`crate::Setup::slices`].
#[derive(Debug, Clone)]
pub struct Slices {
    patterns: Vec<Pattern>,
}

impl Slices {
    pub(crate) fn new(patterns: Vec<Pattern>) -> Slices {
        Slices { patterns }
    }

    /// The slice of a unit: the shortest prefix of its path that a slice pattern matches
    /// exactly. `None`: the unit is in no slice.
    pub fn of<'u>(&self, unit: &'u [String]) -> Option<&'u [String]> {
        slice_of(unit, &self.patterns).map(|length| &unit[..length])
    }
}

/// The length of the shortest prefix of `unit` that matches a slice pattern exactly.
pub(crate) fn slice_of(unit: &[String], slices: &[Pattern]) -> Option<usize> {
    (1..=unit.len()).find(|&length| {
        slices
            .iter()
            .any(|pattern| pattern.matches(&unit[..length]))
    })
}

/// The path of the node that a unit belongs to.
pub(crate) fn node_path<'a>(unit: &'a [String], slices: Option<&Slices>) -> Option<&'a [String]> {
    match slices {
        None => Some(unit),
        Some(slices) => slices.of(unit),
    }
}

/// The paths of the nodes, in the order of the first unit that belongs to each.
pub(crate) fn nodes(facts: &FactSet, selected: &[bool], slices: Option<&Slices>) -> Vec<Path> {
    let mut nodes: Vec<Path> = Vec::new();
    let mut known: FastSet<&[String]> = FastSet::default();
    for (unit, _) in facts
        .units
        .iter()
        .zip(selected)
        .filter(|(_, &selected)| selected)
    {
        if let Some(path) = node_path(unit, slices) {
            if known.insert(path) {
                nodes.push(path.to_vec());
            }
        }
    }
    nodes
}

/// Nodes with a path each, and the edges between them. A node is given by its number.
#[derive(Debug, Clone)]
pub struct Graph<'a> {
    facts: &'a FactSet,
    nodes: Cow<'a, [Path]>,
    edges: Vec<Vec<Edge<'a>>>,
}

/// Nodes that reach each other, with one of their cycles. See [`Graph::tangles`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tangle {
    /// The nodes of the tangle, in ascending order.
    pub members: Vec<usize>,
    /// A shortest cycle through the first member: each step is a node and the number of the
    /// edge of that node that the cycle takes.
    pub cycle: Vec<(usize, usize)>,
}

/// A shortest chain of edges from a node to an end. See [`Search::chain`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    /// Each step is a node and the number of the edge of that node that the chain takes.
    /// Empty: the start is an end.
    pub steps: Vec<(usize, usize)>,
    /// The end that the chain leads to.
    pub end: usize,
}

impl<'a> Graph<'a> {
    /// A graph from the paths of its nodes and, for each node, its edges: see
    /// [`crate::Scope::edges`].
    ///
    /// # Panics
    ///
    /// When the number of nodes is not the number of lists of edges.
    pub fn new(
        facts: &'a FactSet,
        nodes: impl Into<Cow<'a, [Path]>>,
        edges: Vec<Vec<Edge<'a>>>,
    ) -> Graph<'a> {
        let nodes = nodes.into();
        assert_eq!(nodes.len(), edges.len(), "each node has a list of edges");
        Graph {
            facts,
            nodes,
            edges,
        }
    }

    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph has no node.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The path of a node.
    pub fn node(&self, node: usize) -> &Path {
        &self.nodes[node]
    }

    /// The edges of a node, in the order of their first references.
    pub fn edges(&self, node: usize) -> &[Edge<'a>] {
        &self.edges[node]
    }

    /// The fact of the reference that makes an edge, by the node and the number of its edge.
    pub fn fact(&self, from: usize, edge: usize) -> &'a Fact {
        &self.facts.facts[self.edges[from][edge].fact]
    }

    /// The step that an edge gives in a report, by the node and the number of its edge.
    pub fn step(&self, from: usize, edge: usize) -> Step {
        let fact = self.fact(from, edge);
        let edge = &self.edges[from][edge];
        let What::Reference(reference) = &fact.what else {
            unreachable!("an edge comes from a reference")
        };
        Step {
            from: self.nodes[from].clone(),
            to: self.nodes[edge.to].clone(),
            file: fact.file.clone(),
            line: fact.line,
            column: fact.column,
            target: edge.target.to_vec(),
            kind: reference.kind.clone(),
        }
    }

    /// The tangles: each group of two or more nodes that all reach each other, once, in the
    /// order of their first nodes (SPEC 4.5).
    pub fn tangles(&self) -> Vec<Tangle> {
        let components = components(&self.edges);
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); self.len()];
        for (node, &component) in components.iter().enumerate() {
            members[component].push(node);
        }
        let mut tangles = Vec::new();
        for start in 0..self.len() {
            // A tangle is reported at its first node: after that, its list is empty.
            let members = &mut members[components[start]];
            if members.len() < 2 {
                continue;
            }
            let cycle = shortest_cycle(start, &self.edges, &components);
            tangles.push(Tangle {
                members: std::mem::take(members),
                cycle,
            });
        }
        tangles
    }

    /// For each node: a number that the nodes of its tangle share, or `None` when the node is
    /// in no tangle.
    pub(crate) fn tangle_of(&self) -> Vec<Option<usize>> {
        let components = components(&self.edges);
        let mut sizes = vec![0usize; self.len()];
        for &component in &components {
            sizes[component] += 1;
        }
        components
            .into_iter()
            .map(|component| (sizes[component] > 1).then_some(component))
            .collect()
    }

    /// For each node: whether it is a root, or a chain of edges leads from a root to it.
    pub fn reached_from(&self, is_root: impl Fn(usize) -> bool) -> Vec<bool> {
        let mut reached: Vec<bool> = (0..self.len()).map(is_root).collect();
        let mut queue: VecDeque<usize> = (0..reached.len()).filter(|&node| reached[node]).collect();
        while let Some(node) = queue.pop_front() {
            for edge in &self.edges[node] {
                if !reached[edge.to] {
                    reached[edge.to] = true;
                    queue.push_back(edge.to);
                }
            }
        }
        reached
    }

    /// A search for the shortest chains to the nodes that are an end. It is made once for
    /// all starts.
    pub fn search(&self, is_end: impl Fn(usize) -> bool) -> Search<'_, 'a> {
        let ends: Vec<bool> = (0..self.len()).map(is_end).collect();
        let distance = distance_to_end(&self.edges, &ends);
        Search {
            graph: self,
            ends,
            distance,
            found_by: vec![None; self.len()],
            found: Vec::new(),
        }
    }
}

/// The shortest chains of a graph to its ends. See [`Graph::search`].
#[derive(Debug)]
pub struct Search<'g, 'a> {
    graph: &'g Graph<'a>,
    ends: Vec<bool>,
    /// For each node: the number of edges of a shortest chain to an end, or `NEVER`.
    distance: Vec<usize>,
    /// For each node that the last search found: the node and the edge that led to it.
    found_by: Vec<Option<(usize, usize)>>,
    found: Vec<usize>,
}

impl Search<'_, '_> {
    /// A shortest chain from `start` to an end. The search is breadth-first with the edges
    /// of each node in their order, so the result does not depend on chance. `None`: no
    /// chain leads from the node to an end.
    pub fn chain(&mut self, start: usize) -> Option<Chain> {
        // A search from a node that no chain leads from to an end finds nothing.
        if self.distance[start] == NEVER {
            return None;
        }
        for node in self.found.drain(..) {
            self.found_by[node] = None;
        }
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            if self.ends[node] {
                let mut steps = Vec::new();
                let mut current = node;
                while let Some(step) = self.found_by[current] {
                    steps.push(step);
                    current = step.0;
                }
                steps.reverse();
                return Some(Chain { steps, end: node });
            }
            for (number, edge) in self.graph.edges[node].iter().enumerate() {
                // The search is breadth-first, so it ends at a nearest end. A node that is
                // not one step nearer to an end is on no shortest chain, and the nodes on
                // the shortest chains keep their order without it.
                if self.distance[edge.to] + 1 == self.distance[node]
                    && self.found_by[edge.to].is_none()
                {
                    self.found_by[edge.to] = Some((node, number));
                    self.found.push(edge.to);
                    queue.push_back(edge.to);
                }
            }
        }
        unreachable!("a chain leads from the node to an end")
    }
}

/// The distance of a node from which no path of edges leads to an end.
const NEVER: usize = usize::MAX - 1;

/// For each node: the number of edges of a shortest path from it to an end, or `NEVER`.
fn distance_to_end(edges: &[Vec<Edge<'_>>], ends: &[bool]) -> Vec<usize> {
    let mut sources: Vec<Vec<usize>> = vec![Vec::new(); edges.len()];
    for (from, list) in edges.iter().enumerate() {
        for edge in list {
            sources[edge.to].push(from);
        }
    }
    let mut distance: Vec<usize> = ends
        .iter()
        .map(|&end| if end { 0 } else { NEVER })
        .collect();
    let mut queue: VecDeque<usize> = (0..edges.len())
        .filter(|&node| distance[node] == 0)
        .collect();
    while let Some(node) = queue.pop_front() {
        for &source in &sources[node] {
            if distance[source] == NEVER {
                distance[source] = distance[node] + 1;
                queue.push_back(source);
            }
        }
    }
    distance
}

/// The strongly connected component of each node, as a number. Tarjan's algorithm without
/// recursion, so that a long chain of nodes does not use up the call stack.
fn components(edges: &[Vec<Edge<'_>>]) -> Vec<usize> {
    const NONE: usize = usize::MAX;
    let count = edges.len();
    let mut index = vec![NONE; count];
    let mut low = vec![0; count];
    let mut on_stack = vec![false; count];
    let mut component = vec![NONE; count];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_index = 0;
    let mut next_component = 0;

    for root in 0..count {
        if index[root] != NONE {
            continue;
        }
        // Each entry is a node and the number of its edges that are done.
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;

        while let Some(&(node, done)) = work.last() {
            if let Some(edge) = edges[node].get(done) {
                work.last_mut().expect("not empty").1 += 1;
                let to = edge.to;
                if index[to] == NONE {
                    index[to] = next_index;
                    low[to] = next_index;
                    next_index += 1;
                    stack.push(to);
                    on_stack[to] = true;
                    work.push((to, 0));
                } else if on_stack[to] {
                    low[node] = low[node].min(index[to]);
                }
                continue;
            }
            work.pop();
            if low[node] == index[node] {
                loop {
                    let member = stack.pop().expect("the node is on the stack");
                    on_stack[member] = false;
                    component[member] = next_component;
                    if member == node {
                        break;
                    }
                }
                next_component += 1;
            }
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[node]);
            }
        }
    }
    component
}

/// A shortest cycle through `start`, as pairs of node and edge number. Breadth-first, with
/// the edges of each node in their order, so the result does not depend on chance.
fn shortest_cycle(
    start: usize,
    edges: &[Vec<Edge<'_>>],
    components: &[usize],
) -> Vec<(usize, usize)> {
    // For each node that the search reached: the node and the edge that led to it.
    let mut reached_by: Vec<Option<(usize, usize)>> = vec![None; edges.len()];
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for (number, edge) in edges[node].iter().enumerate() {
            if edge.to == start {
                let mut path = vec![(node, number)];
                let mut current = node;
                while let Some(step) = reached_by[current] {
                    path.push(step);
                    current = step.0;
                }
                path.reverse();
                return path;
            }
            if components[edge.to] == components[start] && reached_by[edge.to].is_none() {
                reached_by[edge.to] = Some((node, number));
                queue.push_back(edge.to);
            }
        }
    }
    unreachable!("every node of a tangle is on a cycle")
}

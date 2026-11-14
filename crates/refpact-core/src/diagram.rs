//! Diagrams of the graph that a rule sees (SPEC 4.17): the subjects of the rule, its slices,
//! or a set of layers, and the references between them after the filters and the exemptions.

use std::cell::RefCell;

use crate::check::{Check, Checker, Scope, Setup};
use crate::error::{Role, RuleError};
use crate::eval::Prepared;
use crate::graph::{Graph, Slices};
use crate::model::RawPattern;
use crate::pattern::Pattern;
use crate::report::PathFormat;
use crate::rule::Rule;
use crate::violation::Violation;

/// The text form of a diagram.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiagramFormat {
    /// A Mermaid flowchart. Markdown viewers that know Mermaid draw it from a fenced block
    /// ` ```mermaid `.
    #[default]
    Mermaid,
    /// A Graphviz graph in the DOT language.
    Dot,
}

impl DiagramFormat {
    /// The format as the specification writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            DiagramFormat::Mermaid => "mermaid",
            DiagramFormat::Dot => "dot",
        }
    }
}

/// A layer of a diagram of layers: see [`DiagramNodes::Layers`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramLayer {
    /// The name of the layer, which is the label of its node.
    pub name: String,
    /// The units of the layer: unit patterns, which match units exactly.
    pub units: Vec<RawPattern>,
    /// The layers that the layer must not use, by their position in the list of layers. An
    /// edge to one of them is drawn in red. A position outside the list is ignored.
    pub must_not_use: Vec<usize>,
}

/// What the nodes of a diagram are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiagramNodes<'a> {
    /// One node for each subject of the rule.
    #[default]
    Subjects,
    /// One node for each slice of the subjects, as for `must_be_acyclic` with slices.
    Slices(&'a [RawPattern]),
    /// One node for each layer, from the top to the bottom. A subject is in the first layer
    /// that has a pattern that matches it, and in no node when no layer matches it.
    Layers(&'a [DiagramLayer]),
}

/// The color of what a diagram marks: a cycle, or a use that the layers forbid.
const MARK_COLOR: &str = "#d62728";

impl Prepared {
    /// The diagram of what a rule sees (SPEC 4.17). The nodes are given by `nodes`. An edge
    /// is a reference that the rule sees between two nodes. The constraint of the rule is not
    /// evaluated.
    ///
    /// For subjects and slices, the nodes and edges of a cycle are drawn in red, from left to
    /// right. For layers, the edges to a layer that a layer must not use are drawn in red,
    /// from the top to the bottom.
    ///
    /// The errors are those of the subjects, the filters, the exemptions, and the slices or
    /// the patterns of the layers.
    pub fn diagram(
        &self,
        rule: &Rule,
        nodes: DiagramNodes<'_>,
        format: DiagramFormat,
    ) -> Result<String, Vec<RuleError>> {
        let draw = Draw {
            name: &rule.name,
            nodes,
            format,
            path_format: PathFormat::of(self.facts()),
            text: RefCell::new(String::new()),
        };
        self.evaluate_check(rule, &draw)?;
        Ok(draw.text.into_inner())
    }
}

/// The check that draws the diagram instead of finding violations.
struct Draw<'r> {
    name: &'r str,
    nodes: DiagramNodes<'r>,
    format: DiagramFormat,
    path_format: PathFormat,
    text: RefCell<String>,
}

/// The valid nodes of a diagram, from the setup.
enum ValidNodes<'r> {
    Graph(Option<Slices>),
    Layers(&'r [DiagramLayer], Vec<Vec<Pattern>>),
}

impl Check for Draw<'_> {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        let valid = match self.nodes {
            DiagramNodes::Subjects => ValidNodes::Graph(None),
            DiagramNodes::Slices(raw) => ValidNodes::Graph(Some(setup.slices(raw))),
            DiagramNodes::Layers(layers) => {
                let patterns = layers
                    .iter()
                    .map(|layer| setup.units(&layer.units, Role::Layer))
                    .collect();
                ValidNodes::Layers(layers, patterns)
            }
        };
        Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
            let picture = match &valid {
                ValidNodes::Graph(slices) => {
                    graph_picture(&scope.graph(slices.as_ref()), &self.path_format)
                }
                ValidNodes::Layers(layers, patterns) => layer_picture(scope, layers, patterns),
            };
            *self.text.borrow_mut() = match self.format {
                DiagramFormat::Mermaid => mermaid(self.name, &picture),
                DiagramFormat::Dot => dot(self.name, &picture),
            };
            Vec::new()
        })
    }
}

/// What a diagram draws, whatever its format.
struct Picture {
    /// The direction of the edges: `LR` from left to right, `TB` from the top to the bottom.
    direction: &'static str,
    /// The label of each node.
    labels: Vec<String>,
    /// Whether each node is marked in red.
    marked: Vec<bool>,
    /// The edges in order, each as its two nodes and whether it is marked in red.
    edges: Vec<(usize, usize, bool)>,
}

/// The picture of a graph of subjects or slices: the cycles are marked.
fn graph_picture(graph: &Graph<'_>, format: &PathFormat) -> Picture {
    let tangle = graph.tangle_of();
    let mut edges = Vec::new();
    for from in 0..graph.len() {
        for edge in graph.edges(from) {
            let cycle = tangle[from].is_some() && tangle[from] == tangle[edge.to];
            edges.push((from, edge.to, cycle));
        }
    }
    Picture {
        direction: "LR",
        labels: (0..graph.len())
            .map(|node| format.print(graph.node(node)))
            .collect(),
        marked: tangle.iter().map(Option::is_some).collect(),
        edges,
    }
}

/// The picture of the layers: the uses that the layers forbid are marked.
fn layer_picture(scope: &Scope<'_>, layers: &[DiagramLayer], patterns: &[Vec<Pattern>]) -> Picture {
    let units = &scope.facts().units;
    let layer_of = |unit: usize| {
        if !scope.selects(unit) {
            return None;
        }
        let matches =
            |list: &Vec<Pattern>| list.iter().any(|pattern| pattern.matches(&units[unit]));
        patterns.iter().position(matches)
    };
    let mut edges = Vec::new();
    for (from, list) in scope.edges(layers.len(), layer_of).iter().enumerate() {
        for edge in list {
            let forbidden = layers[from].must_not_use.contains(&edge.to);
            edges.push((from, edge.to, forbidden));
        }
    }
    Picture {
        direction: "TB",
        labels: layers.iter().map(|layer| layer.name.clone()).collect(),
        marked: vec![false; layers.len()],
        edges,
    }
}

/// A Mermaid flowchart, with the name of the rule as its title.
fn mermaid(name: &str, picture: &Picture) -> String {
    let mut text = format!(
        "---\ntitle: \"{}\"\n---\nflowchart {}\n",
        name.replace('\\', "\\\\").replace('"', "\\\""),
        picture.direction
    );
    for (node, label) in picture.labels.iter().enumerate() {
        text.push_str(&format!("    n{node}[\"{}\"]\n", mermaid_label(label)));
    }
    for &(from, to, _) in &picture.edges {
        text.push_str(&format!("    n{from} --> n{to}\n"));
    }
    let marked: Vec<String> = (0..picture.labels.len())
        .filter(|&node| picture.marked[node])
        .map(|node| format!("n{node}"))
        .collect();
    if !marked.is_empty() {
        text.push_str(&format!(
            "    classDef cycle stroke:{MARK_COLOR},stroke-width:2px\n    class {} cycle\n",
            marked.join(",")
        ));
    }
    let links: Vec<String> = picture
        .edges
        .iter()
        .enumerate()
        .filter(|(_, &(_, _, mark))| mark)
        .map(|(number, _)| number.to_string())
        .collect();
    if !links.is_empty() {
        text.push_str(&format!(
            "    linkStyle {} stroke:{MARK_COLOR},stroke-width:2px\n",
            links.join(",")
        ));
    }
    text
}

/// A label of a Mermaid node, which is in double quotes: Mermaid writes characters with a
/// meaning there as `#name;`.
fn mermaid_label(text: &str) -> String {
    let mut label = String::new();
    for char in text.chars() {
        match char {
            '#' => label.push_str("#35;"),
            '"' => label.push_str("#quot;"),
            '<' => label.push_str("#lt;"),
            '>' => label.push_str("#gt;"),
            other => label.push(other),
        }
    }
    label
}

/// A Graphviz digraph, with the name of the rule as its label.
fn dot(name: &str, picture: &Picture) -> String {
    let mut text = format!(
        "digraph {{\n    label=\"{}\";\n    rankdir={};\n    node [shape=box];\n",
        dot_text(name),
        picture.direction
    );
    for (node, label) in picture.labels.iter().enumerate() {
        let color = if picture.marked[node] {
            format!(", color=\"{MARK_COLOR}\"")
        } else {
            String::new()
        };
        text.push_str(&format!(
            "    n{node} [label=\"{}\"{color}];\n",
            dot_text(label)
        ));
    }
    for &(from, to, mark) in &picture.edges {
        let color = if mark {
            format!(" [color=\"{MARK_COLOR}\"]")
        } else {
            String::new()
        };
        text.push_str(&format!("    n{from} -> n{to}{color};\n"));
    }
    text.push_str("}\n");
    text
}

/// Text in a DOT string in double quotes.
fn dot_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_escape_what_mermaid_reads() {
        assert_eq!(mermaid_label("a#\"<b>"), "a#35;#quot;#lt;b#gt;");
        assert_eq!(dot_text("a\\\"b"), "a\\\\\\\"b");
    }
}

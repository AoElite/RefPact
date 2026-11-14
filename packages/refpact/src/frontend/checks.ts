/**
 * Switches for the tests of this package. Each one makes a short way of the front end also
 * take the long way, and a difference between the two is an error.
 *
 * - `aliases`: the first step of an alias is read from the exports of the module. With the
 *   switch, it is compared with the answer of the compiler for that name.
 * - `walk`: a node whose text has no word that can name a module is not visited. With the
 *   switch, all nodes are also visited, and the two visits must find the same.
 *
 * @internal
 */
export const checks = { aliases: false, walk: false };

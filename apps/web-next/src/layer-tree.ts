// Spec 056: QGIS-style layer tree. Groups are "/"-separated paths saved per layer in the project map;
// a group sits where its first layer sits in render order, so moving layers moves their group.
export type LayerTreeNode =
  | { kind: "layer"; id: string }
  | { kind: "group"; path: string; name: string; children: LayerTreeNode[] };

export function buildLayerTree(order: readonly string[], groupOf: (id: string) => string | undefined): LayerTreeNode[] {
  const root: LayerTreeNode[] = [];
  const groups = new Map<string, Extract<LayerTreeNode, { kind: "group" }>>();
  for (const id of order) {
    const parts = (groupOf(id) ?? "").split("/").map(p => p.trim()).filter(Boolean);
    let siblings = root;
    let path = "";
    for (const name of parts) {
      path = path ? path + "/" + name : name;
      let g = groups.get(path);
      if (!g) {
        g = { kind: "group", path, name, children: [] };
        groups.set(path, g);
        siblings.push(g);
      }
      siblings = g.children;
    }
    siblings.push({ kind: "layer", id });
  }
  return root;
}

export function layerIds(node: LayerTreeNode): string[] {
  return node.kind === "layer" ? [node.id] : node.children.flatMap(layerIds);
}

/** Layers of every group path, depth-first, in tree order (used for Shift range selection and "expand all"). */
export function flatten(nodes: readonly LayerTreeNode[]): LayerTreeNode[] {
  return nodes.flatMap(n => (n.kind === "group" ? [n, ...flatten(n.children)] : [n]));
}

/** Rename a group path prefix ("A/B" → "A/C") for every layer below it; returns only changed layers. */
export function renameGroup(groups: Readonly<Record<string, string>>, from: string, to: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [id, path] of Object.entries(groups)) {
    if (path === from) out[id] = to;
    else if (path.startsWith(from + "/")) out[id] = to + path.slice(from.length);
  }
  return out;
}

/** Parent path of a group ("A/B" → "A", "A" → null): ungrouping moves members one level up. */
export const parentPath = (path: string) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : null);

/** Shift selection and keyboard navigation operate on visible rows, never collapsed/filtered rows. */
export function visibleLayerIds(nodes:readonly LayerTreeNode[], expanded:(path:string)=>boolean, included:(id:string)=>boolean):string[]{
 return nodes.flatMap(node=>node.kind==="layer"?(included(node.id)?[node.id]:[]):expanded(node.path)?visibleLayerIds(node.children,expanded,included):[]);
}
export function layerRange(ids:readonly string[],anchor:string,target:string):string[]{
 const a=ids.indexOf(anchor),b=ids.indexOf(target);
 return a<0||b<0?(b<0?[]:[target]):ids.slice(Math.min(a,b),Math.max(a,b)+1);
}
/** Mixed checkbox makes the whole group visible; fully visible checkbox hides it. */
export function groupVisibilityChanges(ids:readonly string[],hidden:ReadonlySet<string>):string[]{
 const invisible=ids.filter(id=>hidden.has(id));
 return invisible.length?invisible:[...ids];
}

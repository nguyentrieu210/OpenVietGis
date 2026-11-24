// Experimental adapter boundary. No API client, credentials or writes.
const families = new Set(["Point","LineString","Polygon"]);
const pinKeys = ["organizationId","projectId","layerId","sourceId","contentRevision","schemaRevision","family"];
const revision = /^[a-f0-9]{64}$/;
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const identity = /^[A-Za-z0-9_-]{1,128}$/;
function checkScope(scope) {
  if (!scope || scope.editable !== true || scope.crs !== 4326 ||
      !families.has(scope.family) ||
      !["organizationId","projectId","layerId","sourceId"].every(k=>typeof scope[k]==="string" && identity.test(scope[k])) ||
      !["contentRevision","schemaRevision"].every(k=>typeof scope[k]==="string" && revision.test(scope[k]))) {
    throw Error("Unsupported or unauthorized draft scope");
  }
}
function point(p) {
  if (!Array.isArray(p) || p.length!==2 || !p.every(Number.isFinite) || Math.abs(p[0])>180 || Math.abs(p[1])>90) throw Error("Invalid WGS84 coordinate");
  return [p[0],p[1]];
}
const equal=(a,b)=>a[0]===b[0]&&a[1]===b[1];
function line(c,min) {
  if(!Array.isArray(c)||c.length<min||c.length>10000)throw Error("Unsupported vertex count");
  const points=c.map(point);
  if(points.slice(1).some((v,i)=>equal(v,points[i])))throw Error("Duplicate consecutive vertex");
  return points;
}
export function draftGeometry(g,family) {
  if(!g||g.type!==family||!families.has(family))throw Error("Geometry family mismatch");
  if(family==="Point")return {type:family,coordinates:point(g.coordinates)};
  if(family==="LineString")return {type:family,coordinates:line(g.coordinates,2)};
  // The first candidate is intentionally limited to simple single-ring polygons.
  if(!Array.isArray(g.coordinates)||g.coordinates.length!==1)throw Error("Polygon holes require the legacy editor");
  const ring=line(g.coordinates[0],4);
  if(!equal(ring[0],ring.at(-1)))throw Error("Polygon ring is not closed");
  if(new Set(ring.slice(0,-1).map(p=>JSON.stringify(p))).size!==ring.length-1)throw Error("Repeated polygon vertex");
  return {type:family,coordinates:[ring]};
}
export function createDraft(scope,featureId,operationId) {
  checkScope(scope);
  if(typeof featureId!=="string"||!identity.test(featureId)||typeof operationId!=="string"||!uuid.test(operationId))throw Error("Invalid operation identity");
  const pinned=Object.freeze({...scope});
  let stopped=false,proposal=null;
  return Object.freeze({
    dispose(){stopped=true;proposal=null},
    prepare(current,geometry,properties){
      if(stopped)throw Error("Draft disposed");
      checkScope(current);
      if(pinKeys.some(k=>current[k]!==pinned[k]))throw Error("Scope or revision changed");
      const safeGeometry=draftGeometry(geometry,pinned.family);
      if(!properties||typeof properties!=="object"||Array.isArray(properties))throw Error("Invalid attributes");
      // Host validates typed schema before calling this adapter. Only JSON scalar values are accepted.
      if(Object.values(properties).some(v=>v!==null&&typeof v!=="string"&&typeof v!=="boolean"&&!(typeof v==="number"&&Number.isFinite(v))))throw Error("Invalid attribute value");
      const candidate={operation_id:operationId,base_content_revision:pinned.contentRevision,base_schema_revision:pinned.schemaRevision,
        summary:"R031 experimental drawing",operations:[{op_type:"create",feature_id:featureId,geometry:safeGeometry,properties:structuredClone(properties)}]};
      // Once proposed, only an identical payload may be retried under this operation ID.
      const canonical=JSON.stringify(candidate);
      if(proposal!==null&&proposal!==canonical)throw Error("Pending operation payload changed");
      proposal=canonical;
      return JSON.parse(canonical);
    }
  });
}

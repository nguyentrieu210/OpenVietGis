//! Clean-room, dependency-free parser for the SPEC 074 logical resource identifiers.
//! A parsed ID is NOT an authorization grant. Never pass URL paths to filesystem APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    TileJson, VectorTile { z: u8, x: u32, y: u32 },
    RasterTile { z: u8, x: u32, y: u32 },
    CadChunk(u32), PointNode(u32), Asset,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub workspace: &'a str,
    pub source: &'a str,
    pub revision: u64,
    pub resource: Resource,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error { Invalid, Unsupported }

fn uuid(s: &str) -> bool {
    s.len() == 36 && s.bytes().enumerate().all(|(i,b)| {
        if [8,13,18,23].contains(&i) { b == b'-' }
        else { b.is_ascii_digit() || (b'a'..=b'f').contains(&b) }
    })
}
fn decimal<T: std::str::FromStr>(s: &str) -> Result<T, Error> {
    if s.is_empty() || s.len() > 20 || (s.len() > 1 && s.starts_with('0'))
        || !s.bytes().all(|b| b.is_ascii_digit()) { return Err(Error::Invalid); }
    s.parse().map_err(|_| Error::Invalid)
}
fn tile(z: &str, x: &str, y: &str, ext: &str) -> Result<(u8,u32,u32),Error> {
    let z: u8 = decimal(z)?;
    if z > 22 { return Err(Error::Invalid); }
    let x: u32 = decimal(x)?;
    let y: u32 = decimal(y.strip_suffix(ext).ok_or(Error::Invalid)?)?;
    if x >= (1_u32 << z) || y >= (1_u32 << z) { return Err(Error::Invalid); }
    Ok((z,x,y))
}
pub fn parse(uri: &str) -> Result<Request<'_>, Error> {
    if uri.len() > 512 || !uri.bytes().all(|b|
        b.is_ascii_alphanumeric() || matches!(b, b':'|b'/'|b'-'|b'.')
    ) { return Err(Error::Invalid); }
    let path = uri.strip_prefix("ovg://v1/").ok_or(Error::Invalid)?;
    let p: Vec<&str> = path.split('/').collect();
    if p.iter().any(|s| s.is_empty() || *s == "." || *s == "..") { return Err(Error::Invalid); }
    let ["workspaces", ws, family, id, "revisions", rev, tail @ ..] = p.as_slice()
        else { return Err(Error::Invalid); };
    if !uuid(ws) || !uuid(id) { return Err(Error::Invalid); }
    let revision = decimal(rev)?;
    let resource = match (*family, tail) {
        ("layers", ["tilejson"]) => Resource::TileJson,
        ("layers", ["tiles", z,x,y]) => { let (z,x,y)=tile(z,x,y,".mvt")?; Resource::VectorTile {z,x,y} },
        ("rasters", ["tiles", z,x,y]) => { let (z,x,y)=tile(z,x,y,".png")?; Resource::RasterTile {z,x,y} },
        ("cad", ["chunks", n]) => Resource::CadChunk(decimal(n)?),
        ("pointcloud", ["nodes", n]) => Resource::PointNode(decimal(n)?),
        ("assets", ["content"]) => Resource::Asset,
        _ => return Err(Error::Unsupported),
    };
    Ok(Request { workspace: ws, source: id, revision, resource })
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: &str="00000000-0000-4000-8000-000000000001";
    const B: &str="00000000-0000-4000-8000-000000000002";
    fn u(t: &str) -> String { format!("ovg://v1/workspaces/{A}/{t}") }
    fn l(t: &str) -> String { u(&format!("layers/{B}/revisions/7/{t}")) }
    #[test] fn accepts_typed_routes() {
        assert_eq!(parse(&l("tilejson")).unwrap().resource, Resource::TileJson);
        assert_eq!(parse(&l("tiles/2/3/1.mvt")).unwrap().resource,Resource::VectorTile{z:2,x:3,y:1});
        assert_eq!(parse(&u(&format!("rasters/{B}/revisions/1/tiles/0/0/0.png"))).unwrap().resource,Resource::RasterTile{z:0,x:0,y:0});
        assert_eq!(parse(&u(&format!("cad/{B}/revisions/1/chunks/2"))).unwrap().resource,Resource::CadChunk(2));
        assert_eq!(parse(&u(&format!("pointcloud/{B}/revisions/1/nodes/3"))).unwrap().resource,Resource::PointNode(3));
        assert_eq!(parse(&u(&format!("assets/{B}/revisions/1/content"))).unwrap().resource,Resource::Asset);
    }
    #[test] fn rejects_ambiguous_and_out_of_range() {
        for bad in ["tiles/23/0/0.mvt","tiles/0/1/0.mvt","tiles/2/0/4.mvt","tiles/02/0/0.mvt","tiles/2/0/-1.mvt","tiles/2/0/0.png","tiles/2/0/0.mvt?token=x","tilejson#x","%2e%2e","../tilejson","//tilejson","tilejson/","tiles/2/0/99999999999999999999.mvt"] {
            assert!(parse(&l(bad)).is_err(), "{bad}");
        }
        assert!(parse(&l("tilejson").replace(A,"C:/windows")).is_err());
        assert!(parse(&l("tilejson").replace("/7/","/18446744073709551616/")).is_err());
        assert!(parse(&l("tilejson").replace("/7/","/07/")).is_err());
        assert!(parse(&l("tilejson").replace("ovg://v1","https://example.org")).is_err());
        assert!(parse(&"x".repeat(513)).is_err());
    }
    #[test] fn no_panic_for_adversarial_ascii() {
        let base = l("tiles/2/0/0.mvt");
        for i in 0..base.len() {
            for c in 0_u8..=127 {
                let mut bytes=base.as_bytes().to_vec(); bytes[i]=c;
                let _=parse(std::str::from_utf8(&bytes).unwrap());
            }
        }
    }
}

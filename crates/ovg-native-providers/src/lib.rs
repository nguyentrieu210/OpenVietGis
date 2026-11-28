//! Optional local native adapters. No network, arbitrary SQL or renderer paths.
#[cfg(feature="ogr")]
pub mod ogr;
#[cfg(feature="analytics")]
pub mod analytics;
pub type Result<T>=std::result::Result<T,&'static str>;

#[cfg(feature="remote-range")]
pub mod http_range;

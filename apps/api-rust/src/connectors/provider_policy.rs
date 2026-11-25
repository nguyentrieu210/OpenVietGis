//! Bounded immutable COG range validation, ported from the Python reference.
//! This is not a transport: caller must independently enforce tenant ACL,
//! approved provider profile, pinned DNS, TLS, deadline and bounded reads.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_OBJECT: u64 = 1 << 31;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unavailable;

pub fn strong_etag(value: &str) -> bool {
    let bytes = value.as_bytes();
    (3..=130).contains(&bytes.len())
        && bytes[0] == b'"'
        && bytes[bytes.len() - 1] == b'"'
        && bytes[1..bytes.len() - 1]
            .iter()
            .all(|&b| b == 0x21 || (0x23..=0x7e).contains(&b))
}

#[derive(Debug, PartialEq, Eq)]
pub struct CogRange<'a> {
    first: u64,
    last: u64,
    pinned: Option<(&'a str, u64)>,
}

impl<'a> CogRange<'a> {
    pub fn new(first: u64, last: u64, pinned: Option<(&'a str, u64)>) -> Result<Self, Unavailable> {
        if first > last || last >= MAX_OBJECT || last - first + 1 > MAX_BYTES {
            return Err(Unavailable);
        }
        if let Some((tag, total)) = pinned {
            if !strong_etag(tag) || first >= total || total > MAX_OBJECT {
                return Err(Unavailable);
            }
        }
        Ok(Self {
            first,
            last,
            pinned,
        })
    }

    pub fn limit(&self) -> u64 {
        self.last - self.first + 1
    }

    /// Validate response metadata before reading the body. Header arguments
    /// must be unique; the transport must reject duplicates.
    pub fn validate_headers<'b>(
        &self,
        response: CogResponseHeaders<'b>,
    ) -> Result<ValidatedCogRange<'b>, Unavailable> {
        if response.status != 206
            || response.location_present
            || !response
                .encoding
                .unwrap_or("identity")
                .trim()
                .eq_ignore_ascii_case("identity")
            || !["image/tiff", "image/geotiff", "application/geotiff"]
                .iter()
                .any(|m| {
                    response
                        .media_type
                        .split(';')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .eq_ignore_ascii_case(m)
                })
        {
            return Err(Unavailable);
        }
        let etag = response
            .etag
            .filter(|tag| strong_etag(tag))
            .ok_or(Unavailable)?;
        let range = response.content_range.ok_or(Unavailable)?;
        let (span, size) = range
            .strip_prefix("bytes ")
            .and_then(|s| s.split_once('/'))
            .ok_or(Unavailable)?;
        let (first, last) = span.split_once('-').ok_or(Unavailable)?;
        let decimal = |s: &str| -> Result<u64, Unavailable> {
            if s.is_empty() || s.len() > 10 || !s.bytes().all(|c| c.is_ascii_digit()) {
                return Err(Unavailable);
            }
            s.parse().map_err(|_| Unavailable)
        };
        let (first, last, total) = (decimal(first)?, decimal(last)?, decimal(size)?);
        if first != self.first
            || first >= total
            || total > MAX_OBJECT
            || last != self.last.min(total - 1)
        {
            return Err(Unavailable);
        }
        if let Some((tag, size)) = self.pinned {
            if tag != etag || size != total {
                return Err(Unavailable);
            }
        }
        let length = last - first + 1;
        if response.content_length != Some(length) || length > self.limit() {
            return Err(Unavailable);
        }
        Ok(ValidatedCogRange {
            total,
            etag,
            length,
        })
    }
}

#[derive(Clone, Copy)]
pub struct CogResponseHeaders<'a> {
    pub status: u16,
    pub location_present: bool,
    pub encoding: Option<&'a str>,
    pub media_type: &'a str,
    pub etag: Option<&'a str>,
    pub content_range: Option<&'a str>,
    pub content_length: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedCogRange<'a> {
    total: u64,
    etag: &'a str,
    length: u64,
}

impl<'a> ValidatedCogRange<'a> {
    /// The transport can cap the read at one byte beyond this bounded length
    /// and must then validate the received length before returning any bytes.
    pub fn expected_body_len(&self) -> u64 {
        self.length
    }

    pub fn validate_body_len(self, received: u64) -> Result<(u64, &'a str), Unavailable> {
        if received != self.length {
            return Err(Unavailable);
        }
        Ok((self.total, self.etag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response() -> CogResponseHeaders<'static> {
        CogResponseHeaders {
            status: 206,
            location_present: false,
            encoding: None,
            media_type: "image/tiff",
            etag: Some("\"v1\""),
            content_range: Some("bytes 0-9/10"),
            content_length: Some(10),
        }
    }
    #[test]
    fn accepts_exact_and_clipped_eof() {
        assert_eq!(
            CogRange::new(0, 9, None)
                .unwrap()
                .validate_headers(response())
                .and_then(|range| range.validate_body_len(10)),
            Ok((10, "\"v1\""))
        );
        assert_eq!(
            CogRange::new(0, 19, Some(("\"v1\"", 10)))
                .unwrap()
                .validate_headers(response())
                .and_then(|range| range.validate_body_len(10)),
            Ok((10, "\"v1\""))
        );
    }
    #[test]
    fn rejects_invalid_requests() {
        assert!(CogRange::new(1, 0, None).is_err());
        assert!(CogRange::new(0, MAX_BYTES, None).is_err());
        assert!(CogRange::new(MAX_OBJECT, MAX_OBJECT, None).is_err());
        assert!(CogRange::new(10, 20, Some(("\"v1\"", 10))).is_err());
        assert!(CogRange::new(0, 9, Some(("W/\"v1\"", 10))).is_err());
    }
    #[test]
    fn rejects_bad_response() {
        let req = CogRange::new(0, 9, Some(("\"v1\"", 10))).unwrap();
        for r in [
            CogResponseHeaders {
                status: 200,
                ..response()
            },
            CogResponseHeaders {
                location_present: true,
                ..response()
            },
            CogResponseHeaders {
                encoding: Some("gzip"),
                ..response()
            },
            CogResponseHeaders {
                media_type: "text/html",
                ..response()
            },
            CogResponseHeaders {
                etag: Some("\"v2\""),
                ..response()
            },
            CogResponseHeaders {
                etag: Some("W/\"v1\""),
                ..response()
            },
            CogResponseHeaders {
                content_range: Some("bytes 0-9/11"),
                ..response()
            },
            CogResponseHeaders {
                content_range: Some("bytes 1-9/10"),
                ..response()
            },
            CogResponseHeaders {
                content_range: Some("bytes 0-10/10"),
                ..response()
            },
            CogResponseHeaders {
                content_range: Some("bytes 0-9/*"),
                ..response()
            },
            CogResponseHeaders {
                content_length: None,
                ..response()
            },
            CogResponseHeaders {
                content_length: Some(11),
                ..response()
            },
        ] {
            assert_eq!(req.validate_headers(r), Err(Unavailable));
        }
    }
    #[test]
    fn rejects_short_and_oversized_bodies_after_validating_headers() {
        let range = CogRange::new(0, 19, None)
            .unwrap()
            .validate_headers(response())
            .unwrap();
        assert_eq!(range.expected_body_len(), 10);
        assert_eq!(range.validate_body_len(9), Err(Unavailable));
        assert_eq!(range.validate_body_len(11), Err(Unavailable));
    }

    #[test]
    fn shared_python_reference_cases() {
        for line in include_str!("../../../../fixtures/064-e12/cog-ranges.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            let fields: Vec<_> = line.split('|').collect();
            let num = |i: usize| fields[i].parse::<u64>().unwrap();
            let opt = |i: usize| (!fields[i].is_empty()).then_some(fields[i]);
            let pin = opt(4).map(|tag| (tag, num(5)));
            let request = CogRange::new(num(2), num(3), pin).unwrap();
            let response = CogResponseHeaders {
                status: num(6) as u16,
                location_present: opt(9).is_some(),
                encoding: opt(8),
                media_type: fields[7],
                etag: opt(11),
                content_range: opt(10),
                content_length: Some(num(12)),
            };
            assert_eq!(
                request
                    .validate_headers(response)
                    .and_then(|range| range.validate_body_len(num(12)))
                    .is_ok(),
                fields[1] == "1",
                "{}",
                fields[0]
            );
        }
    }

    #[test]
    fn validates_strong_ascii_etags() {
        for value in ["\"a\"", "\"!#~\"", "\"a\\b\""] {
            assert!(strong_etag(value));
        }
        for value in ["", "\"\"", "W/\"a\"", "\"a b\"", "\"a\"b\"", "\"é\""] {
            assert!(!strong_etag(value));
        }
        assert!(strong_etag(&format!("\"{}\"", "a".repeat(128))));
        assert!(!strong_etag(&format!("\"{}\"", "a".repeat(129))));
    }
}

//! W0 qualification adapter: calls the existing portable spatial kernel.
use ovg_spatial::{Coord2D, PortableTransformKernel, TransformBackend};

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

/// Bounded batch, explicit source/target CRS, XY axis order.
/// Reject invalid data and unsupported operations without fallback.
pub fn transform_batch(
    source_epsg: u32,
    target_epsg: u32,
    xy: Vec<f64>,
) -> Result<Vec<f64>, String> {
    if xy.is_empty() || xy.len() % 2 != 0 || xy.len() > 8192 {
        return Err("INVALID_BATCH_SIZE".to_owned());
    }
    let kernel = PortableTransformKernel::new();
    let mut output = Vec::with_capacity(xy.len());
    for point in xy.chunks_exact(2) {
        let transformed = kernel
            .transform_point(source_epsg, target_epsg, Coord2D::new(point[0], point[1]))
            .map_err(|e| e.to_string())?;
        output.push(transformed.x);
        output.push(transformed.y);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_kernel_transform_and_invalid_input() {
        let result = transform_batch(4326, 3857, vec![1.0, 0.0]).unwrap();
        assert!((result[0] - 111319.49079327357).abs() < 0.001);
        assert!(result[1].abs() < 0.001);
        assert!(transform_batch(4326, 999999, vec![1.0, 0.0]).is_err());
        assert!(transform_batch(4326, 3857, vec![f64::NAN, 0.0]).is_err());
        assert!(transform_batch(4326, 3857, vec![0.0; 8194]).is_err());
        assert!(transform_batch(4326, 3857, vec![1.0]).is_err());
    }
}

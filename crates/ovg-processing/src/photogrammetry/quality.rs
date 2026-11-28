//! Initial designated high-overlap survey profile, SPEC 076 R2.
//! Evaluates measured metrics; does not establish fixture provenance or a signed receipt.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Failure { NonFinite, InvalidPopulation, Registration, Reprojection, Checkpoints, XyRmse, ZRmse, XyP95, ZP95, Coverage, Seam, Color }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurveyMetrics {
    pub input_images:u64, pub registered_images:u64, pub checkpoint_count:u64,
    pub gsd_m:f64, pub median_reprojection_px:f64,
    pub xy_rmse_m:f64, pub z_rmse_m:f64, pub xy_p95_m:f64, pub z_p95_m:f64,
    pub coverage_fraction:f64, pub seam_p95_px:f64, pub median_delta_e00:f64,
}
pub fn survey_failures(m:SurveyMetrics)->Vec<Failure>{
    let values=[m.gsd_m,m.median_reprojection_px,m.xy_rmse_m,m.z_rmse_m,m.xy_p95_m,m.z_p95_m,
        m.coverage_fraction,m.seam_p95_px,m.median_delta_e00];
    if values.iter().any(|v|!v.is_finite() || *v<0.0) {return vec![Failure::NonFinite];}
    if m.gsd_m==0.0 || m.input_images==0 || m.registered_images>m.input_images || m.coverage_fraction>1.0 {
        return vec![Failure::InvalidPopulation];
    }
    let mut out=Vec::new();
    if (m.registered_images as u128)*100 < (m.input_images as u128)*95 {out.push(Failure::Registration);}
    if m.median_reprojection_px>1.0 {out.push(Failure::Reprojection);}
    if m.checkpoint_count<10 {out.push(Failure::Checkpoints);}
    if m.xy_rmse_m/m.gsd_m>2.0 {out.push(Failure::XyRmse);}
    if m.z_rmse_m/m.gsd_m>3.0 {out.push(Failure::ZRmse);}
    if m.xy_p95_m/m.gsd_m>4.0 {out.push(Failure::XyP95);}
    if m.z_p95_m/m.gsd_m>6.0 {out.push(Failure::ZP95);}
    if m.coverage_fraction<0.98 {out.push(Failure::Coverage);}
    if m.seam_p95_px>1.0 {out.push(Failure::Seam);}
    if m.median_delta_e00>3.0 {out.push(Failure::Color);}
    out
}
#[cfg(test)]
mod tests{
    use super::*;
    fn good()->SurveyMetrics {SurveyMetrics{input_images:100,registered_images:95,checkpoint_count:10,
        gsd_m:0.025,median_reprojection_px:1.0,xy_rmse_m:0.025,z_rmse_m:0.05,
        xy_p95_m:0.05,z_p95_m:0.1,coverage_fraction:0.98,seam_p95_px:1.0,median_delta_e00:3.0}}
    #[test] fn passes_designated_fixture_thresholds(){assert!(survey_failures(good()).is_empty());}
    #[test] fn poor_geometry_cannot_pass(){let mut m=good();m.checkpoint_count=9;m.xy_rmse_m=0.1;m.seam_p95_px=2.0;
        assert_eq!(survey_failures(m),vec![Failure::Checkpoints,Failure::XyRmse,Failure::Seam]);}
    #[test] fn invalid_metrics_fail_closed(){let mut m=good();m.gsd_m=f64::NAN;
        assert_eq!(survey_failures(m),vec![Failure::NonFinite]);m=good();m.registered_images=101;
        assert_eq!(survey_failures(m),vec![Failure::InvalidPopulation]);}
}

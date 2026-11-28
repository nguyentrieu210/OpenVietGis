//! Bounded reference CPU orthorectification on an existing surface and calibrated cameras.
//! Input images must be undistorted. No occlusion handling or seam optimization yet.
#[derive(Debug,Clone,Copy)]
pub struct Camera {pub world_to_camera:[[f64;3];3],pub center:[f64;3],pub fx:f64,pub fy:f64,pub cx:f64,pub cy:f64}
#[derive(Debug,Clone,Copy)]
pub struct Grid {pub origin_x:f64,pub origin_y:f64,pub gsd:f64,pub width:u32,pub height:u32}
pub struct Image<'a>{pub camera:Camera,pub width:u32,pub height:u32,pub rgb:&'a [u8]}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit}
fn dot(a:[f64;3],b:[f64;3])->f64{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
impl Camera{
    pub(crate) fn valid(self)->bool{
        let r=self.world_to_camera;
        let det=r[0][0]*(r[1][1]*r[2][2]-r[1][2]*r[2][1])-r[0][1]*(r[1][0]*r[2][2]-r[1][2]*r[2][0])+r[0][2]*(r[1][0]*r[2][1]-r[1][1]*r[2][0]);
        self.center.iter().chain([self.fx,self.fy,self.cx,self.cy].iter()).all(|x|x.is_finite())&&self.fx>0.&&self.fy>0.&&
        r.iter().flatten().all(|x|x.is_finite())&&(det-1.).abs()<1e-6&&
        (0..3).all(|i|(0..3).all(|j|(dot(r[i],r[j])-if i==j{1.}else{0.}).abs()<1e-6))
    }
    fn project(self,p:[f64;3])->Option<([f64;2],f64)>{
        let v=[p[0]-self.center[0],p[1]-self.center[1],p[2]-self.center[2]];
        let x=dot(self.world_to_camera[0],v);let y=dot(self.world_to_camera[1],v);let z=dot(self.world_to_camera[2],v);
        if !z.is_finite()||z<=1e-9{return None;}
        let uv=[self.fx*x/z+self.cx,self.fy*y/z+self.cy];
        if !uv.iter().all(|x|x.is_finite()){return None;}
        // Prefer near-optical-axis views. Not a visibility or surface-normal test.
        let score=z/(x.hypot(y).hypot(z));
        Some((uv,score))
    }
}
fn sample(im:&Image<'_>,uv:[f64;2])->Option<[u8;3]>{
    let [u,v]=uv;
    if u<0.||v<0.||u>f64::from(im.width-1)||v>f64::from(im.height-1){return None;}
    let x=u.floor() as usize;let y=v.floor() as usize;
    let x1=(x+1).min(im.width as usize-1);let y1=(y+1).min(im.height as usize-1);
    let a=u-x as f64;let b=v-y as f64;
    let mut out=[0;3];let w=im.width as usize;
    for (k,o) in out.iter_mut().enumerate(){
        let p=|xx,yy|f64::from(im.rgb[(yy*w+xx)*3+k]);
        *o=((1.-b)*((1.-a)*p(x,y)+a*p(x1,y))+b*((1.-a)*p(x,y1)+a*p(x1,y1))).round().clamp(0.,255.) as u8;
    }
    Some(out)
}
/// Output RGBA in row-major order, alpha=0 on missing surface or image support.
/// Pixel centers use north-up grid origin. Finite DEM elevations in the same
/// metric frame as cameras are required; NaN means no data.
pub fn render(grid:Grid,dem:&[f32],images:&[Image<'_>],max_output_bytes:u64,max_samples:u64)->Result<Vec<u8>,Error>{
    if grid.width==0||grid.height==0||!grid.gsd.is_finite()||grid.gsd<=0.||!grid.origin_x.is_finite()||
        !grid.origin_y.is_finite()||images.is_empty()||images.len()>256{return Err(Error::Invalid);}
    let n=u64::from(grid.width)*u64::from(grid.height);
    let bytes=n.checked_mul(4).ok_or(Error::Limit)?;
    if bytes>max_output_bytes||bytes>256*1024*1024||
        n.checked_mul(images.len() as u64).ok_or(Error::Limit)?>max_samples{return Err(Error::Limit);}
    if dem.len() as u64!=n{return Err(Error::Invalid);}
    for im in images{
        let expected=u64::from(im.width).checked_mul(u64::from(im.height)).and_then(|n|n.checked_mul(3));
        if im.width==0||im.height==0||expected!=Some(im.rgb.len() as u64)||!im.camera.valid(){return Err(Error::Invalid);}
    }
    let mut out=vec![0u8;bytes as usize];
    for (i,&height) in dem.iter().enumerate(){
        if !height.is_finite(){continue;}
        let x=i%grid.width as usize;let y=i/grid.width as usize;
        let p=[grid.origin_x+(x as f64+0.5)*grid.gsd,grid.origin_y-(y as f64+0.5)*grid.gsd,f64::from(height)];
        let mut best=-1.;
        for im in images{
            if let Some((uv,score))=im.camera.project(p){
                if score>best{if let Some(rgb)=sample(im,uv){out[i*4..i*4+3].copy_from_slice(&rgb);out[i*4+3]=255;best=score;}}
            }
        }
    }
    Ok(out)
}
#[cfg(test)]mod tests{
    use super::*;
    fn camera()->Camera{Camera{world_to_camera:[[1.,0.,0.],[0.,-1.,0.],[0.,0.,-1.]],center:[0.,0.,10.],fx:10.,fy:10.,cx:0.,cy:0.}}
    #[test]fn projects_and_samples_nadir(){
        let im=Image{camera:camera(),width:2,height:2,rgb:&[0,0,0,100,0,0,0,100,0,100,100,0]};
        let grid=Grid{origin_x:0.,origin_y:0.,gsd:1.,width:1,height:1};
        assert_eq!(render(grid,&[0.],&[im],4,1).unwrap(),vec![50,50,0,255]);
    }
    #[test]fn nodata_and_limits(){
        let im=Image{camera:camera(),width:1,height:1,rgb:&[1,2,3]};
        let grid=Grid{origin_x:0.,origin_y:0.,gsd:1.,width:1,height:1};
        assert_eq!(render(grid,&[f32::NAN],&[im],4,1).unwrap(),vec![0;4]);
        let im=Image{camera:camera(),width:1,height:1,rgb:&[1,2,3]};
        assert_eq!(render(grid,&[0.],&[im],3,1),Err(Error::Limit));
    }
}

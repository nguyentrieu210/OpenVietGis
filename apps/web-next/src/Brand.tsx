import React from "react";
import "./brand.css";

export function Brand(){
 return <div className="brand" aria-label="OpenVietGIS Studio">
  <img className="logo brand-mark" src={import.meta.env.BASE_URL+"brand/ovg-mark.png"} width={40} height={40} alt=""/>
  <div className="brand-wordmark"><b>OpenVietGIS</b><small>STUDIO</small></div>
 </div>;
}

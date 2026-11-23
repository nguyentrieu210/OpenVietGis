import type {ReactNode} from "react";
import type {Geometry,Feature,FeatureCollection,Position} from "geojson";
export type StudioFeatureCollection=FeatureCollection;
export type StudioPosition=Position;
import type {ApiClient,SpatialLayer,SpatialFeature} from "@openvietgis/client-sdk";
export type StudioRow={key:string;layer:SpatialLayer;feature:SpatialFeature;color:string;geometry:Geometry|null};
export type StudioData={layers:SpatialLayer[];rows:StudioRow[];warnings:string[];permissions:string[];restricted?:boolean};
export type StudioHost={
 client:ApiClient;org:string;pid:string;data:StudioData;row:StudioRow|undefined;selected:string;
 loading:boolean;busy:boolean;editing:boolean;revision:number;
 action:(task:()=>Promise<void>)=>Promise<void>;refresh:()=>void;fail:(e:unknown)=>void;
 select:(key:string)=>void;goto:(key:string)=>void;fit:()=>void;resetLayers:()=>void;notify:(message:string)=>void;
};
export type StudioSearchResult={key:string;label:string;detail?:string};
export type StudioLayerGroup={label:string;color:string;layers:SpatialLayer[];countLabel:string};
export type StudioContribution={
 mapRevision?:unknown;toolbar?:ReactNode;panel?:ReactNode;inspector?:ReactNode;modal?:ReactNode;legend?:ReactNode;
 layerGroups?:StudioLayerGroup[];layerColor?:(layer:SpatialLayer,fallback:string)=>string;filter?:(row:StudioRow)=>boolean;
 paint?:(row:StudioRow)=>Partial<{color:string;selected:boolean;opacity:number;strokeWidth:number}>;
 rank?:(row:StudioRow)=>number;label?:(row:StudioRow)=>string|undefined;
 focus?:(collection:FeatureCollection,visible:StudioRow[])=>Feature[];
 search?:(query:string)=>Promise<StudioSearchResult[]>;searchPlaceholder?:string;
 reset?:()=>void;
};
export type StudioPlugin={id:string;useStudio:(host:StudioHost)=>StudioContribution};
export const featureKey=(source:string|null|undefined,feature:string|null|undefined)=>source&&feature?source+":"+feature:"";
export function useEmptyStudio(_host:StudioHost):StudioContribution{return {}}

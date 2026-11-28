import React from "react";
export type IconName="add"|"save"|"reload"|"undo"|"redo"|"hand"|"select"|"plus"|"minus"|"extent"|"layer"|"up"|"down"|"settings"|"table"|"edit"|"point"|"line"|"polygon"|"ruler"|"area"|"query"|"buffer"|"clip"|"export"|"plugin"|"chevron"|"folder"|"database"|"eye"|"close"|"search"|"more"|"globe"|"lock"|"layers"|"info"|"eye-off";
const paths:Record<IconName,React.ReactNode>={
"eye-off":<><path d="M3 3l16 16M4 7c-2 2-3 4-3 4s4 7 10 7c2 0 4-1 6-2M8 4c1-.4 2-.5 3-.5 6 0 10 7.5 10 7.5s-1 2-3 4"/></>,
add:<><rect x="3" y="4" width="13" height="13" rx="2"/><path d="M13 7h5M15.5 4.5v5M6 13l3-3 3 3 2-2"/></>,
save:<><path d="M3 3h13l3 3v13H3z"/><path d="M7 3v6h8V3M7 19v-7h8v7"/></>,
reload:<><path d="M17 7a7 7 0 1 0 1 8M17 3v5h-5"/></>,
undo:<path d="m8 4-5 5 5 5M3 9h10a5 5 0 0 1 0 10h-3"/>,redo:<path d="m14 4 5 5-5 5m5-5H9a5 5 0 0 0 0 10h3"/>,
hand:<path d="M6 11V6a1.5 1.5 0 0 1 3 0v4-7a1.5 1.5 0 0 1 3 0v7-5a1.5 1.5 0 0 1 3 0v6-3a1.5 1.5 0 0 1 3 0v6c0 4-2 6-6 6h-1c-3 0-4-2-6-5l-2-3c-1-2 1-3 2-2z"/>,
select:<path d="M5 2l13 10-6 1-3 6-4-17z"/>,
plus:<><circle cx="9" cy="9" r="6"/><path d="m14 14 5 5M6 9h6M9 6v6"/></>,minus:<><circle cx="9" cy="9" r="6"/><path d="m14 14 5 5M6 9h6"/></>,
extent:<><path d="M8 3H3v5M14 3h5v5M3 14v5h5M19 14v5h-5"/><circle cx="11" cy="11" r="3"/></>,
layer:<><path d="m2 7 9-5 9 5-9 5zM3 12l8 5 8-5M3 17l8 4 8-4"/></>,
up:<path d="M11 18V4M5 10l6-6 6 6"/>,down:<path d="M11 4v14M5 12l6 6 6-6"/>,
settings:<><circle cx="11" cy="11" r="3"/><path d="m11 2 2 3 3-1 2 2-1 3 3 2-3 2 1 3-2 2-3-1-2 3-2-3-3 1-2-2 1-3-3-2 3-2-1-3 2-2 3 1z"/></>,
table:<><rect x="2" y="3" width="18" height="16" rx="1"/><path d="M2 8h18M2 13h18M8 8v11M14 8v11"/></>,
edit:<><path d="m4 15-1 4 4-1L19 6l-4-4zM12 5l4 4"/></>,
point:<><circle cx="11" cy="11" r="5"/><path d="M11 2v4M11 16v4M2 11h4M16 11h4"/></>,
line:<><path d="m3 17 7-12 9 9"/><rect x="1" y="15" width="4" height="4" fill="#fff"/><rect x="8" y="3" width="4" height="4" fill="#fff"/><rect x="17" y="12" width="4" height="4" fill="#fff"/></>,
polygon:<><path d="m3 5 11-2 6 10-8 7-9-5z" fill="#c7e6c2"/><circle cx="3" cy="5" r="1.5"/><circle cx="14" cy="3" r="1.5"/><circle cx="20" cy="13" r="1.5"/><circle cx="12" cy="20" r="1.5"/><circle cx="3" cy="15" r="1.5"/></>,
ruler:<><path d="m2 15 13-13 5 5-13 13z" fill="#fde6a2"/><path d="m6 11 3 3m1-7 3 3m1-7 3 3"/></>,area:<><rect x="3" y="3" width="16" height="16" strokeDasharray="3 2" fill="#dce9c4"/><path d="m6 16 10-10M6 6h10v10"/></>,
query:<><path d="M3 3h16l-6 7v8l-4 2V10z" fill="#d0e7ed"/></>,buffer:<><circle cx="11" cy="11" r="9" fill="#d3edcf"/><circle cx="11" cy="11" r="4"/></>,clip:<><rect x="3" y="3" width="11" height="11" fill="#d7e5ec"/><rect x="8" y="8" width="11" height="11" fill="#c5e7c8" fillOpacity=".7"/></>,export:<><path d="M11 2v13M6 10l5 5 5-5M3 14v6h16v-6"/></>,
plugin:<><path d="M3 7h5V4a3 3 0 1 1 6 0v3h5v5h-2a3 3 0 1 0 0 6h2v3H3v-5h2a3 3 0 1 0 0-6H3z"/></>,
chevron:<path d="m8 5 6 6-6 6"/>,folder:<path d="M2 5h7l2 3h9v11H2z" fill="#f2cf78"/>,database:<><ellipse cx="11" cy="5" rx="8" ry="3" fill="#d3e9c2"/><path d="M3 5v12c0 4 16 4 16 0V5M3 11c0 4 16 4 16 0"/></>,
eye:<><path d="M2 11s3-6 9-6 9 6 9 6-3 6-9 6-9-6-9-6z"/><circle cx="11" cy="11" r="3"/></>,close:<path d="m5 5 12 12M17 5 5 17"/>,search:<><circle cx="9" cy="9" r="6"/><path d="m14 14 5 5"/></>,more:<><circle cx="4" cy="11" r="1"/><circle cx="11" cy="11" r="1"/><circle cx="18" cy="11" r="1"/></>,globe:<><circle cx="11" cy="11" r="9"/><ellipse cx="11" cy="11" rx="4" ry="9"/><path d="M2 11h18"/></>,lock:<><rect x="5" y="9" width="12" height="11" rx="2"/><path d="M8 9V6a3 3 0 0 1 6 0v3"/></>,layers:<><rect x="3" y="3" width="16" height="16"/><path d="M8 3v16M3 8h5M3 13h5"/></>,info:<><circle cx="11" cy="11" r="9"/><path d="M11 10v5M11 7v.1"/></>};
export function GisIcon({name,size=19}:{name:IconName;size?:number}){return <svg width={size} height={size} viewBox="0 0 22 22" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>}

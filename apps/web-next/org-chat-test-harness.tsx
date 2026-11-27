import React from "react";
import {createRoot} from "react-dom/client";
import {ApiClient} from "@openvietgis/client-sdk";
import {OrgChat} from "./src/OrgChat";
createRoot(document.getElementById("root")!).render(<OrgChat client={new ApiClient({baseUrl:"/api/v1"})} org="org-test" accountId="me" close={()=>{}}/>);

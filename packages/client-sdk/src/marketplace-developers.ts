import type {ApiClient} from "./http";
import type {MarketplaceRelease} from "./external-plugins";

export type DeveloperPublisher={id:string;name:string;public_key:string;created_at:string;key_revision:number};
export type PublisherRegistration={name:string;public_key:string;proof:string};
export type SignedSubmission=Pick<MarketplaceRelease,"publisher_id"|"manifest"|"signature">;
export type SubmissionStatus="pending"|"approved"|"rejected"|"withdrawn";
export type DeveloperSubmission=MarketplaceRelease&{
 id:string;public_key:string;status:SubmissionStatus;review_note:string|null;reviewed_at:string|null;
 created_at:string;own:boolean;published:boolean;
};
export type DeveloperState={user_id:string;publisher:DeveloperPublisher|null;submissions:DeveloperSubmission[];
 can_review:boolean;catalog_available:boolean;quota:number};
const root="/developer/marketplace";
const post=(body:unknown)=>({method:"POST",body:JSON.stringify(body)});
export const getDeveloperState=(c:ApiClient)=>c.request<DeveloperState>(root+"/me",{cache:"no-store"});
export const registerDeveloperPublisher=(c:ApiClient,body:PublisherRegistration)=>c.request<DeveloperPublisher>(root+"/publisher",post(body));
export const submitDeveloperRelease=(c:ApiClient,body:SignedSubmission)=>c.request<DeveloperSubmission>(root+"/submissions",post(body));
export const withdrawDeveloperRelease=(c:ApiClient,id:string)=>c.request<DeveloperSubmission>(root+"/submissions/"+encodeURIComponent(id)+"/withdraw",{method:"POST"});
export const listMarketplaceReviews=(c:ApiClient,state:SubmissionStatus,offset=0)=>c.request<{items:DeveloperSubmission[];next_offset:number|null;catalog_available:boolean}>(root+"/review-queue?state="+encodeURIComponent(state)+"&offset="+offset,{cache:"no-store"});
export const decideMarketplaceReview=(c:ApiClient,id:string,decision:"approved"|"rejected",note:string)=>c.request<DeveloperSubmission>(root+"/review-queue/"+encodeURIComponent(id)+"/decision",post({decision,note}));

export type DeveloperV2Submission={submission_id:string;status:SubmissionStatus;release_digest:string;review_note:string|null;parent_status:SubmissionStatus;publisher_id:string;extension_key:string;version:string;own:boolean;withdrawal_requested:boolean;withdrawal_reason:string|null;bundle?:unknown};
const v2=root+"/v2/submissions";
const v2Path=(id:string)=>v2+"/"+encodeURIComponent(id);
export const listDeveloperV2=(c:ApiClient,review=false,status:SubmissionStatus|"all"="all",offset=0)=>c.request<{items:DeveloperV2Submission[];next_offset:number|null}>(v2+"?review="+review+"&status="+encodeURIComponent(status)+"&offset="+offset,{cache:"no-store"});
export const getDeveloperV2=(c:ApiClient,id:string)=>c.request<DeveloperV2Submission>(v2Path(id),{cache:"no-store"});
export const attachDeveloperV2=(c:ApiClient,id:string,bundle:unknown)=>c.request<DeveloperV2Submission>(v2Path(id),post(bundle));
export const decideDeveloperV2=(c:ApiClient,id:string,decision:"approved"|"rejected",note:string)=>c.request<DeveloperV2Submission>(v2Path(id)+"/decision",post({decision,note}));
export const withdrawDeveloperV2=(c:ApiClient,id:string,reason:string)=>c.request<DeveloperV2Submission>(v2Path(id)+"/withdraw",post({reason}));
export const exportDeveloperV2=(c:ApiClient,id:string)=>c.request<{bundle:unknown;publisher_public_key:string;release_digest:string;requires_offline_operator_signature:true}>(v2Path(id)+"/export",{cache:"no-store"});

export type PublisherRotation={publisher_id:string;expected_revision:number;old_public_key:string;new_public_key:string;old_proof:string;new_proof:string};
export const rotateDeveloperPublisherKey=(c:ApiClient,body:PublisherRotation)=>c.request<DeveloperPublisher>(root+"/publisher/rotate-key",post(body));

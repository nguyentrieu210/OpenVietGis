import type {ApiClient} from "./http";
import type {MarketplaceRelease} from "./external-plugins";

export type DeveloperPublisher={id:string;name:string;public_key:string;created_at:string};
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

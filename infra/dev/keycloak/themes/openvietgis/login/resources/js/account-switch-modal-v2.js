/* A forced re-authentication for GIS must allow a different account.
 * Use Keycloak's own reset-login action rather than changing session cookies.
 */
(() => {
  const switchAccount = () => {
    const query = new URLSearchParams(window.location.search);
    if (query.get("prompt") !== "login" || query.get("client_id") !== "openvietgis-web") return;
    const redirect = query.get("redirect_uri");
    if (redirect !== window.location.origin + "/api/v1/auth/callback/keycloak") return;
    const reset = document.getElementById("reset-login");
    if (reset) reset.click();
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", switchAccount, {once:true});
  else switchAccount();
})();

/* Present the real Keycloak forms inside the same-origin app modal. The OIDC
   transaction and credentials stay in Keycloak; only layout events are sent. */
(() => {
  let embedded=false;
  try { embedded=window.parent!==window && window.parent.location.origin===window.location.origin; } catch {}
  if(!embedded)return;
  document.documentElement.classList.add("ovg-embedded");
  const notify=()=>{
    const body=document.querySelector(".pf-v5-c-login__main");
    if(body)window.parent.postMessage({type:"ovg:auth-height",height:Math.ceil(body.getBoundingClientRect().height)+3},window.location.origin);
  };
  const start=()=>{
    notify();
    const main=document.querySelector(".pf-v5-c-login__main");
    if(main&&typeof ResizeObserver!=="undefined")new ResizeObserver(notify).observe(main);
    const social=document.getElementById("kc-social-providers"), form=document.getElementById("kc-form");
    if(social&&form&&form.parentElement&&document.getElementById("kc-login")){
      form.parentElement.insertBefore(social,form);
      social.classList.add("ovg-social-first");
      const divider=document.createElement("div");
      divider.className="ovg-email-divider";divider.textContent="Hoặc đăng nhập bằng email";
      social.after(divider);
    }
    for(const link of document.querySelectorAll('#kc-social-providers a, a[id^="social-"]')){
      link.setAttribute("target","_top");
      link.removeAttribute("rel");
      link.addEventListener("click",()=>window.parent.postMessage({type:"ovg:social-start"},window.location.origin));
    }
  };
  if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",start,{once:true});
  else start();
})();


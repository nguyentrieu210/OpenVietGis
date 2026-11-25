import React,{useState} from "react";

// Spec 058: full emoji set for chat (no dependency): categories + recent, click to insert.
const SETS:[string,string,string][]=[
 ["😀","Mặt cười","😀 😃 😄 😁 😆 😅 🤣 😂 🙂 😉 😊 😇 🥰 😍 🤩 😘 😗 😚 😋 😛 😜 🤪 😝 🤑 🤗 🤭 🤫 🤔 🤐 🤨 😐 😑 😶 😏 😒 🙄 😬 😮‍💨 🤥 😌 😔 😪 🤤 😴 😷 🤒 🤕 🤢 🤮 🥵 🥶 🥴 😵 🤯 🤠 🥳 😎 🤓 🧐 😕 😟 🙁 😮 😯 😲 😳 🥺 😦 😧 😨 😰 😥 😢 😭 😱 😖 😣 😞 😓 😩 😫 🥱 😤 😡 😠 🤬 😈 👿 💀 💩 🤡 👻 👽 🤖"],
 ["👍","Cử chỉ","👍 👎 👌 🤌 🤏 ✌️ 🤞 🤟 🤘 🤙 👈 👉 👆 👇 ☝️ ✋ 🤚 🖐️ 🖖 👋 🤝 🙏 👏 🙌 👐 🤲 💪 🦾 ✍️ 💅 🫡 🫶 👀 🧠 🫵"],
 ["❤️","Tim","❤️ 🧡 💛 💚 💙 💜 🖤 🤍 🤎 💔 ❣️ 💕 💞 💓 💗 💖 💘 💝 💯 💢 💥 💫 💦 💨 🔥 ✨ ⭐ 🌟"],
 ["✅","Công việc","✅ ☑️ ✔️ ❌ ❎ ⚠️ ⛔ 🚫 ❓ ❗ ‼️ ⁉️ 🆗 🆕 🆙 🔴 🟠 🟡 🟢 🔵 🟣 ⚫ ⚪ 📌 📍 📎 🔗 📝 📋 📁 📂 🗂️ 📅 📆 ⏰ ⏳ ⌛ 🕐 📊 📈 📉 💼 🖊️ ✏️ 🔒 🔓 🔑 🔔 📣 💬 💡 🛠️ 🔧 🔨 ⚙️ 🧰 🚀 🎯 🏁 🏆 🎉 🎊"],
 ["🗺️","Bản đồ","🗺️ 🧭 🌍 🌏 📍 📌 🏠 🏡 🏘️ 🏢 🏬 🏫 🏥 🏦 🏛️ ⛪ 🏗️ 🧱 🛣️ 🛤️ 🌉 🚧 🚦 🚥 🚗 🚕 🚙 🚌 🛵 🏍️ 🚲 🚶 🧍 🚁 ✈️ 🛰️ 📡 🌳 🌲 🌴 🌾 🌱 🌿 🍃 🌊 💧 🏞️ ⛰️ 🏔️ 🌄 🌅 ☀️ 🌤️ ⛅ 🌧️ ⛈️ 🌩️ ❄️ 🌈"],
 ["🍜","Ăn uống","☕ 🍵 🧋 🥤 🍺 🍻 🥂 🍷 🍜 🍲 🍛 🍚 🍙 🥟 🥖 🍞 🥪 🍔 🍟 🍕 🌭 🍗 🍖 🥩 🍤 🥗 🍉 🍌 🍊 🍎 🥭 🍍 🥥 🍰 🎂 🍦 🍫 🍬"],
];
const RECENT="ovg:emoji-recent";
export function EmojiPicker({pick,close}:{pick:(e:string)=>void;close:()=>void}){
 const [tab,setTab]=useState(0),[recent,setRecent]=useState<string[]>(()=>{try{return JSON.parse(localStorage.getItem(RECENT)??"[]")}catch{return []}});
 const use=(e:string)=>{const r=[e,...recent.filter(x=>x!==e)].slice(0,16);setRecent(r);try{localStorage.setItem(RECENT,JSON.stringify(r))}catch{/* storage unavailable */}pick(e)};
 const list=tab<0?recent:SETS[tab][2].split(" ");
 return <div className="ovg-emoji" role="dialog" aria-label="Chọn biểu tượng" onKeyDown={e=>{if(e.key==="Escape")close()}}>
  <nav>{recent.length>0&&<button type="button" aria-pressed={tab<0} title="Dùng gần đây" onClick={()=>setTab(-1)}>🕘</button>}{SETS.map(([icon,name],i)=><button type="button" key={name} aria-pressed={tab===i} title={name} onClick={()=>setTab(i)}>{icon}</button>)}</nav>
  <small>{tab<0?"Dùng gần đây":SETS[tab][1]}</small>
  <div className="ovg-emoji-grid">{list.map(e=><button type="button" key={e} onClick={()=>use(e)} aria-label={e}>{e}</button>)}</div>
 </div>;
}

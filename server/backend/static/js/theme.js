var t=document.getElementById("theme-toggle"),e=document.documentElement;function n(){return e.classList.contains("dark")}function o(){t.textContent=n()?"\u2600\uFE0F":"\u{1F319}"}t.addEventListener("click",()=>{e.classList.toggle("dark"),localStorage.setItem("ui-schema-theme",n()?"dark":"light"),o()});o();
//# sourceMappingURL=theme.js.map

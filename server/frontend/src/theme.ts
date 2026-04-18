const toggle = document.getElementById("theme-toggle") as HTMLButtonElement;
const root = document.documentElement;

function isDark(): boolean {
  return root.classList.contains("dark");
}

function updateIcon(): void {
  toggle.textContent = isDark() ? "\u2600\uFE0F" : "\uD83C\uDF19";
}

toggle.addEventListener("click", () => {
  root.classList.toggle("dark");
  localStorage.setItem("ui-schema-theme", isDark() ? "dark" : "light");
  updateIcon();
});

updateIcon();

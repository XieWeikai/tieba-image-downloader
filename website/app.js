const installOptions = {
  zh: {
    agent: {
      label: "CLAUDE CODE",
      title: "安装 Skill，然后直接发送贴吧链接",
      command: "/plugin marketplace add XieWeikai/tieba-image-downloader\n/plugin install tieba-image-downloader@tieba-tools",
      hint: "安装后直接说：“下载这个帖子的全部原图”。Skill 会自行安装经过校验的 CLI。",
    },
    release: {
      label: "GITHUB RELEASE",
      title: "下载适合你的 macOS 构建",
      command: "# Apple Silicon\ncurl -LO https://github.com/XieWeikai/tieba-image-downloader/releases/latest",
      hint: "Release 同时提供 Apple Silicon 与 Intel 压缩包，以及 SHA256SUMS 校验清单。",
    },
    source: {
      label: "RUST / CARGO",
      title: "从源码构建并运行最新版本",
      command: "git clone https://github.com/XieWeikai/tieba-image-downloader.git\ncd tieba-image-downloader\n./install-macos.sh",
      hint: "需要 Rust stable、Cargo，以及用于客户端渲染的 Chrome、Chromium 或 Brave。",
    },
  },
  en: {
    agent: {
      label: "CLAUDE CODE",
      title: "Install the skill, then send a Tieba URL",
      command: "/plugin marketplace add XieWeikai/tieba-image-downloader\n/plugin install tieba-image-downloader@tieba-tools",
      hint: "Ask it to download the original images. The skill installs the verified CLI automatically.",
    },
    release: {
      label: "GITHUB RELEASE",
      title: "Download the build for your Mac",
      command: "# Apple Silicon\ncurl -LO https://github.com/XieWeikai/tieba-image-downloader/releases/latest",
      hint: "Releases include Apple Silicon and Intel archives plus a SHA256SUMS manifest.",
    },
    source: {
      label: "RUST / CARGO",
      title: "Build and run the latest source",
      command: "git clone https://github.com/XieWeikai/tieba-image-downloader.git\ncd tieba-image-downloader\n./install-macos.sh",
      hint: "Requires Rust stable, Cargo, and Chrome, Chromium, or Brave for client rendering.",
    },
  },
};

let language = localStorage.getItem("tieba-site-language") || (navigator.language.startsWith("zh") ? "zh" : "en");
let installMode = "agent";

function updateInstall() {
  const option = installOptions[language][installMode];
  document.querySelector("[data-install-label]").textContent = option.label;
  document.querySelector("[data-install-title]").textContent = option.title;
  document.querySelector("[data-command]").textContent = option.command;
  document.querySelector("[data-install-hint]").textContent = option.hint;
}

function setLanguage(nextLanguage) {
  language = nextLanguage;
  document.documentElement.lang = language === "zh" ? "zh-CN" : "en";
  document.querySelectorAll("[data-zh][data-en]").forEach((element) => {
    element.textContent = element.dataset[language];
  });
  document.querySelectorAll("[data-lang]").forEach((button) => {
    button.setAttribute("aria-pressed", String(button.dataset.lang === language));
  });
  localStorage.setItem("tieba-site-language", language);
  updateInstall();
}

document.querySelectorAll("[data-lang]").forEach((button) => {
  button.addEventListener("click", () => setLanguage(button.dataset.lang));
});

document.querySelectorAll("[data-install]").forEach((button) => {
  button.addEventListener("click", () => {
    installMode = button.dataset.install;
    document.querySelectorAll("[data-install]").forEach((tab) => {
      tab.setAttribute("aria-selected", String(tab === button));
    });
    updateInstall();
  });
});

document.querySelector("[data-copy]").addEventListener("click", async (event) => {
  const button = event.currentTarget;
  const original = button.textContent;
  await navigator.clipboard.writeText(document.querySelector("[data-command]").textContent);
  button.textContent = language === "zh" ? "已复制" : "Copied";
  window.setTimeout(() => { button.textContent = original; }, 1400);
});

const header = document.querySelector("[data-header]");
const updateHeader = () => header.classList.toggle("is-scrolled", window.scrollY > 24);
window.addEventListener("scroll", updateHeader, { passive: true });
updateHeader();
setLanguage(language);

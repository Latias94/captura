(function () {
  async function lintRule(yaml) {
    try {
      const resp = await fetch("/ui/rules/lint", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ yaml: yaml }),
      });
      if (!resp.ok) {
        return { ok: false, message: "lint request failed" };
      }
      return await resp.json();
    } catch (e) {
      return { ok: false, message: "lint request error" };
    }
  }

  document.addEventListener("DOMContentLoaded", function () {
    var textarea = document.getElementById("ruleYaml");
    var statusEl = document.getElementById("ruleLintStatus");
    if (!textarea || !statusEl) {
      return;
    }

    var timer = null;
    function scheduleLint() {
      if (timer) {
        clearTimeout(timer);
      }
      timer = setTimeout(async function () {
        var v = textarea.value || "";
        if (!v.trim()) {
          statusEl.textContent = "";
          return;
        }
        statusEl.textContent = "Linting...";
        var res = await lintRule(v);
        if (res.ok) {
          statusEl.textContent = "Lint OK";
          statusEl.classList.remove("text-error");
        } else {
          statusEl.textContent = res.message || "Lint failed";
          statusEl.classList.add("text-error");
        }
      }, 600);
    }

    textarea.addEventListener("input", scheduleLint);
    // Also lint once on load if there is existing YAML.
    if (textarea.value && textarea.value.trim()) {
      scheduleLint();
    }
  });
})();


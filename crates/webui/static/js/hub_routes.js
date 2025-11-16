(function () {
  function buildHubUrlFromButton(btn) {
    var hubId = btn.getAttribute("data-hub-id");
    if (!hubId) {
      return null;
    }
    var paramsRaw = btn.getAttribute("data-hub-params") || "";
    var params = paramsRaw
      .split(",")
      .map(function (s) {
        return s.trim();
      })
      .filter(function (s) {
        return s.length > 0;
      });

    var url = "captura_hub://" + hubId;
    if (params.length > 0) {
      var qs = params
        .map(function (k) {
          return encodeURIComponent(k) + "=";
        })
        .join("&");
      url += "?" + qs;
    }
    return url;
  }

  async function copyHubUrl(btn) {
    var url = buildHubUrlFromButton(btn);
    if (!url) {
      return;
    }
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(url);
        if (window.showToast) {
          window.showToast("Hub URL copied", "info");
        }
      } else {
        // Fallback: prompt so user can copy manually.
        window.prompt("Copy Hub URL", url);
      }
    } catch (e) {
      window.prompt("Copy Hub URL", url);
    }
  }

  document.addEventListener("DOMContentLoaded", function () {
    // Copy buttons
    var buttons = document.querySelectorAll("[data-hub-copy]");
    if (buttons.length) {
      buttons.forEach(function (btn) {
        btn.addEventListener("click", function (e) {
          e.preventDefault();
          copyHubUrl(btn);
        });
      });
    }

    // Simple client-side filter for Hub routes
    var filterInput = document.getElementById("hubFilter");
    if (filterInput) {
      var items = Array.prototype.slice.call(
        document.querySelectorAll("[data-hub-text]")
      );
      filterInput.addEventListener("input", function () {
        var q = filterInput.value.toLowerCase();
        items.forEach(function (li) {
          var text = (li.getAttribute("data-hub-text") || "").toLowerCase();
          if (!q || text.indexOf(q) !== -1) {
            li.style.display = "";
          } else {
            li.style.display = "none";
          }
        });
      });
    }
  });
})();

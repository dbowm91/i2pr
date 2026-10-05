// i2pr console bootstrap and bounded overview refresh.
//
// Deliberately dependency-free and CSP-safe: no eval, no Function
// constructor, no inline handlers, no remote fetches. Every page is fully
// readable with scripting disabled; this only adds refresh on top of the
// server-rendered snapshot.

(function bootstrap() {
  "use strict";

  var doc = document;
  doc.documentElement.setAttribute("data-scripting", "enabled");

  function onReady(fn) {
    if (doc.readyState === "loading") {
      doc.addEventListener("DOMContentLoaded", fn, { once: true });
    } else {
      fn();
    }
  }

  // Reload buttons are plain <button data-console-action="reload">; the
  // server renders the label so the control is meaningful without script.
  function wireActions() {
    var buttons = doc.querySelectorAll("[data-console-action='reload']");
    Array.prototype.forEach.call(buttons, function (button) {
      button.addEventListener("click", function () {
        button.disabled = true;
        window.location.reload();
      });
    });
  }

  // Bounded refresh policy (Plan 358 §9).
  var REFRESH_INTERVAL_MS = 15000;
  var MIN_REFRESH_INTERVAL_MS = 5000;
  var MAX_BACKOFF_MS = 120000;
  var REQUEST_TIMEOUT_MS = 5000;

  var refresh = {
    inFlight: null,
    timer: null,
    backoff: REFRESH_INTERVAL_MS,
    stopped: false
  };

  // One in-flight refresh per page. A newer request aborts the older one,
  // so a stale response can never overwrite fresher state.
  function requestOverview() {
    if (refresh.stopped) {
      return Promise.resolve(null);
    }
    if (refresh.inFlight) {
      refresh.inFlight.abort();
    }
    var controller = new AbortController();
    refresh.inFlight = controller;
    var timeout = window.setTimeout(function () {
      controller.abort();
    }, REQUEST_TIMEOUT_MS);

    return fetch("/api/overview", {
      method: "GET",
      credentials: "same-origin",
      cache: "no-store",
      signal: controller.signal,
      headers: { Accept: "application/json" }
    })
      .then(function (response) {
        if (!response.ok) {
          throw new Error("overview request failed");
        }
        return response.json();
      })
      .then(function (document_) {
        window.clearTimeout(timeout);
        refresh.inFlight = null;
        refresh.backoff = REFRESH_INTERVAL_MS;
        return document_;
      })
      .catch(function () {
        window.clearTimeout(timeout);
        refresh.inFlight = null;
        // Errors back off rather than hammering the router.
        refresh.backoff = Math.min(refresh.backoff * 2, MAX_BACKOFF_MS);
        return null;
      });
  }

  function schedule() {
    if (refresh.stopped) {
      return;
    }
    window.clearTimeout(refresh.timer);
    var delay = Math.max(refresh.backoff, MIN_REFRESH_INTERVAL_MS);
    // A hidden tab stops polling entirely rather than running unseen work.
    if (doc.hidden) {
      delay = MAX_BACKOFF_MS;
    }
    refresh.timer = window.setTimeout(function () {
      requestOverview().then(schedule);
    }, delay);
  }

  // The server-rendered page is authoritative. Polling is an enhancement
  // for freshness, and a stopped poll must never leave the page blank.
  function startRefresh() {
    var root = doc.getElementById("console-metrics");
    if (!root) {
      return;
    }
    schedule();
  }

  // Stop polling when the page goes away, so no request outlives the view.
  window.addEventListener("pagehide", function () {
    refresh.stopped = true;
    window.clearTimeout(refresh.timer);
    if (refresh.inFlight) {
      refresh.inFlight.abort();
    }
  });

  onReady(function () {
    wireActions();
    startRefresh();
  });
})();
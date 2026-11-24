(() => {
  window.neighbourSecret = "loaded";
  window.parent.postMessage({ neighbour: "loaded" }, "*");
})();

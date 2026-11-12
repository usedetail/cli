(function () {
  var hash;
  try {
    hash = decodeURIComponent(window.location.hash);
  } catch (_) {
    hash = "";
  }
  // Scalar has used both #GET/path and #tag/name/GET/path anchors.
  var operation = hash.match(/(?:^#|\/)(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|TRACE)(\/.*)$/);
  var key = operation ? "#" + operation[1] + operation[2] : hash;
  var route = (window.DETAIL_API_REDIRECTS || {})[key];
  if (typeof route !== "string" || !/^\/api\/(?:#[^\s]*)?$/.test(route)) {
    route = "/api/";
  }
  var destination = "https://docs.detail.dev" + route;
  document.getElementById("destination").href = destination;
  window.location.replace(destination);
})();

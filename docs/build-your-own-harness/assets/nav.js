// Highlights the current page in the sidebar based on the URL path.
(function () {
  const here = location.pathname.replace(/\\/g, "/").split("/").pop() || "index.html";
  document.querySelectorAll(".sidebar a").forEach((a) => {
    const href = a.getAttribute("href");
    if (!href) return;
    const tail = href.split("/").pop();
    if (tail === here) a.classList.add("active");
  });
})();

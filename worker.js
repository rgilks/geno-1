export default {
  async fetch(request, env) {
    const response = await env.ASSETS.fetch(request);

    if (!response.ok) return response;

    const res = new Response(response.body, response);

    const url = new URL(request.url);

    if (url.pathname.endsWith(".wasm")) {
      res.headers.set("Cache-Control", "public, max-age=86400");
    } else if (url.pathname.endsWith(".js") || url.pathname.endsWith(".css")) {
      res.headers.set("Cache-Control", "public, max-age=3600");
    } else if (url.pathname === "/" || url.pathname.endsWith(".html")) {
      res.headers.set("Cache-Control", "public, max-age=300");
    }

    return res;
  },
};

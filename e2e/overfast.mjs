import { createServer } from "node:http";

export async function startOverfast() {
  const replies = new Map();
  const requests = [];
  const server = createServer((req, res) => {
    const request = { method: req.method, path: req.url, headers: req.headers, body: "" };
    req.on("data", chunk => { request.body += chunk; });
    requests.push(request);
    const reply = replies.get(req.url) ?? { status: 404 };
    setTimeout(() => {
      res.writeHead(reply.status ?? 200, { "Content-Type": "application/json" });
      res.end(typeof reply.body === "string" ? reply.body : JSON.stringify(reply.body ?? {}));
    }, reply.delay ?? 0).unref();
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  return {
    url: `http://127.0.0.1:${server.address().port}`, requests,
    set(tag, reply) { replies.set(`/players/${tag.replace("#", "-")}/summary`, reply); },
    async stop() { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); },
  };
}

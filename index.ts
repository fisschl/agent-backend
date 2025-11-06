import { Elysia } from "elysia";
import { cors } from "@elysiajs/cors";

new Elysia()
  .use(cors())
  .all("/compatible-mode/v1/*", async ({ request }) => {
    const url = new URL(request.url);
    url.host = "dashscope.aliyuncs.com";
    url.protocol = "https:";

    const { DASHSCOPE_API_KEY } = Bun.env;
    const headers = new Headers(request.headers);
    if (DASHSCOPE_API_KEY)
      headers.set("Authorization", `Bearer ${DASHSCOPE_API_KEY}`);

    const { method } = request;
    const options: RequestInit = { method, headers };

    if (method !== "GET" && request.body) options.body = request.body;

    return fetch(url, options);
  })
  .listen(3000);

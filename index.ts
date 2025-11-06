import { Elysia } from "elysia";
import { cors } from "@elysiajs/cors";

/**
 * 请求头黑名单（需要移除的头）
 */
const REQUEST_HEADERS_BLOCKLIST = [
  "Host",
  "Connection",
  "Keep-Alive",
  "Proxy-Authenticate",
  "Proxy-Authorization",
  "TE",
  "Trailer",
  "Transfer-Encoding",
  "Upgrade",
  "Origin",
  "Referer",
];

/**
 * 响应头黑名单（需要移除的头）
 */
const RESPONSE_HEADERS_BLOCKLIST = [
  "Connection",
  "Keep-Alive",
  "Proxy-Authenticate",
  "Proxy-Authorization",
  "TE",
  "Trailer",
  "Transfer-Encoding",
  "Upgrade",
  "Access-Control-Allow-Origin",
  "Access-Control-Allow-Methods",
  "Access-Control-Allow-Headers",
  "Access-Control-Allow-Credentials",
  "Access-Control-Expose-Headers",
  "Access-Control-Max-Age",
];

new Elysia()
  .use(cors())
  .all("/compatible-mode/v1/*", async ({ request, path, query }) => {
    const target = new URL(path, "https://dashscope.aliyuncs.com");
    const qs = new URLSearchParams(query);
    if (qs) target.search = qs.toString();

    const { headers: requestHeaders } = request;

    REQUEST_HEADERS_BLOCKLIST.forEach((header) =>
      requestHeaders.delete(header)
    );

    const { DASHSCOPE_API_KEY } = Bun.env;
    if (DASHSCOPE_API_KEY)
      requestHeaders.set("Authorization", `Bearer ${DASHSCOPE_API_KEY}`);

    const { method } = request;
    const options: RequestInit = { method, headers: requestHeaders };
    if (method !== "GET" && request.body) options.body = request.body;

    const response = await fetch(target, options);
    const { headers: responseHeaders } = response;

    RESPONSE_HEADERS_BLOCKLIST.forEach((header) =>
      responseHeaders.delete(header)
    );

    return new Response(response.body, {
      status: response.status,
      statusText: response.statusText,
      headers: responseHeaders,
    });
  })
  .listen(3000);

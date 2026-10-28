// Addresses of the commands the back office calls, `POST /api/<model>/<command>`. The back office puts
// `?lang=<language shown>` on every one of them (the query outranks the language cookie), so a matcher
// that compares the whole address, or its end, misses them: match the path, not the query string.

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** For `page.route()`: the address of `<model>.<command>`, with or without a query string. */
export function commandUrl(model: string, command: string): RegExp {
  return new RegExp(`/api/${escape(model)}/${escape(command)}(\\?.*)?$`);
}

/** For a request listener: whether `url` is the address of `<model>.<command>`, whatever its query. */
export function isCommand(url: string, model: string, command: string): boolean {
  return new URL(url).pathname === `/api/${model}/${command}`;
}

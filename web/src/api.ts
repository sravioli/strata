// The calls of the site to the API and to Garage (tech design 0065). Each call gives the status
// code and the body as they are. No call is sent again, except a status read after its Retry-After.
import { ISSUER, accessToken, type Person } from "./auth";

/** The address of make dev. The API listens on IPv4 only. */
export const API = "http://127.0.0.1:8080";

/** The most entries of one batch status read (`status::MAX_BATCH`). */
export const MAX_BATCH = 100;

export interface Answer {
  status: number;
  ok: boolean;
  /** The JSON body, or null. */
  json: any;
  /** The body as text, for an error. */
  text: string;
  /** Retry-After in seconds, or null. */
  retryAfter: number | null;
}

async function answer(r: Response): Promise<Answer> {
  const text = await r.text();
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {}
  const ra = r.headers.get("retry-after");
  return { status: r.status, ok: r.ok, json, text, retryAfter: ra === null ? null : Number(ra) };
}

/** A call to the API with the token in Authorization, and no cookie. */
export async function call(method: string, path: string, body?: unknown): Promise<Answer> {
  const token = await accessToken();
  if (!token) throw new Error("Sign in first.");
  const headers: Record<string, string> = { authorization: `Bearer ${token}` };
  if (body !== undefined) headers["content-type"] = "application/json";
  const r = await fetch(API + path, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    credentials: "omit",
  });
  return answer(r);
}

/** The PUT of the file to the signed URL of Garage. The URL carries the signature. */
export async function put(url: string, file: File): Promise<Answer> {
  const r = await fetch(url, {
    method: "PUT",
    headers: { "content-type": "application/pdf" },
    body: file,
    credentials: "omit",
  });
  return answer(r);
}

/** An error as the page shows it: the status code and the body of the API. */
export function describe(a: Answer): string {
  return `${a.status} ${a.text}`.trim();
}

// ---- The uploads of the site, in localStorage under one key for each issuer, tenant and sub.

export interface Upload {
  document_id: string;
  revision: number;
  file_name: string;
}

function key(p: Person): string {
  return `strata-uploads:${ISSUER}:${p.tenant}:${p.sub}`;
}

export function uploads(p: Person): Upload[] {
  try {
    return JSON.parse(localStorage.getItem(key(p)) ?? "[]");
  } catch {
    return [];
  }
}

export function remember(p: Person, u: Upload): void {
  const list = uploads(p).filter((x) => !(x.document_id === u.document_id && x.revision === u.revision));
  list.push(u);
  localStorage.setItem(key(p), JSON.stringify(list));
}

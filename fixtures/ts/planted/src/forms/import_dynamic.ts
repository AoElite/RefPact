export async function load(): Promise<number> {
  const client = await import("../db/client"); //~ dynamic-import src/db/client.ts
  return client.query("x").length;
}

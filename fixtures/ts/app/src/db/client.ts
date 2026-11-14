export interface Row {
  readonly id: number;
  readonly name: string;
}

export function query(text: string): Row[] {
  return text.length > 0 ? [{ id: 1, name: text }] : [];
}

// The file that the rules forbid.

export interface Row {
  readonly id: number;
}

export class Client {
  rows: Row[] = [];
}

export function query(text: string): Row[] {
  return text.length > 0 ? [{ id: 1 }] : [];
}

export default function connect(): Client {
  return new Client();
}

import { alpha } from "./alpha";

export interface Gamma {
  readonly size: number;
}
export const gamma = (): number => alpha();

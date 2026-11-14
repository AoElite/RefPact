// `alpha` imports `beta`, `beta` imports a type of `gamma`, and `gamma` imports `alpha`.
import { beta } from "./beta";

export const alpha = (): number => beta() + 1;

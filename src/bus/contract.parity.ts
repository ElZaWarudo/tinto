import type { CuratedBusContractTypeMap } from "./contract";
import type { GeneratedBusContractTypeMap } from "./contract.generated";

type Assert<T extends true> = T;
type SameKeys<Left, Right> =
  Exclude<keyof Left, keyof Right> extends never
    ? Exclude<keyof Right, keyof Left> extends never
      ? true
      : false
    : false;

export type BusContractTypeNamesMatch = Assert<
  SameKeys<CuratedBusContractTypeMap, GeneratedBusContractTypeMap>
>;

export type CuratedBusContractAcceptsGenerated = Assert<
  GeneratedBusContractTypeMap extends CuratedBusContractTypeMap ? true : false
>;

export type GeneratedBusContractAcceptsCurated = Assert<
  CuratedBusContractTypeMap extends GeneratedBusContractTypeMap ? true : false
>;

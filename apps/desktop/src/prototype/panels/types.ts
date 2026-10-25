/**
 * PROTOTYPE — throwaway. Variant registry types; one file per panel under this folder.
 */

import type { ComponentType } from "react";

export interface VariantDef {
  key: string;
  name: string;
  /** One line: what this variant is betting on. */
  bet: string;
  /** Picked by the user; becomes the section's default and is marked in the UI. */
  chosen?: boolean;
  Component: ComponentType;
}

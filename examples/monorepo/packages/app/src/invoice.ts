import { subtotal, type Item } from "@demo/core/money";

export function invoiceTotal(items: readonly Item[]): number {
  return subtotal(items);
}

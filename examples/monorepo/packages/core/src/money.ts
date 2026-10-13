export interface Item { unitPrice: number; quantity: number; }

export function subtotal(items: readonly Item[]): number {
  return items.reduce((sum, item) => sum + item.unitPrice * item.quantity, 0);
}

export interface LineItem {
  quantity: number;
  unitPrice: number;
}

/** Calculate a gross total from invoice lines and a tax rate. */
export function invoiceTotal(lines: LineItem[], taxRate: number = 0.19): number {
  const subtotal = lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0);
  return Math.round(subtotal * (1 + taxRate) * 100) / 100;
}

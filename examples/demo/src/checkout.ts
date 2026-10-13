import { invoiceTotal, type LineItem } from './invoice.js';

export function checkout(lines: LineItem[]): { total: number; currency: string } {
  return { total: invoiceTotal(lines), currency: 'EUR' };
}

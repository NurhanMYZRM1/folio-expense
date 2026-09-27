export const CATEGORIES = [
  'Meals',
  'Transport',
  'Accommodation',
  'Fuel',
  'Parking',
  'Office Supplies',
  'Travel',
  'Entertainment',
  'Software',
  'Other',
];
export const STATUS_LABELS: Record<string, string> = {
  draft: 'Draft',
  extracting: 'Extracting',
  needs_review: 'Needs review',
  ready: 'Ready',
  submitted: 'Submitted',
  archived: 'Archived',
  pending: 'Queued',
  running: 'Processing',
  completed: 'Completed',
  failed: 'Needs attention',
};
export const CATEGORY_COLORS = [
  '#2c7968',
  '#6c86af',
  '#d3a65a',
  '#8b76a8',
  '#61a5a2',
  '#b77b82',
  '#73868b',
  '#b99672',
  '#708fbf',
  '#89919c',
];
export function dateLabel(date: string | null): string {
  if (!date) return 'Not set';
  const d = new Date(`${date.slice(0, 10)}T12:00:00`);
  return Number.isNaN(d.valueOf())
    ? date
    : new Intl.DateTimeFormat('en-GB', { day: '2-digit', month: 'short', year: 'numeric' }).format(
        d,
      );
}

import type { ReactNode } from 'react';
import { Circle, LoaderCircle, FileText, ArrowUpRight } from 'lucide-react';
import { Link } from 'react-router-dom';
import { STATUS_LABELS } from '../lib/constants';
import type { FieldMeta } from '../bindings/generated';
export function StatusBadge({ status }: { status: string }) {
  return (
    <span className={`badge ${status}`}>
      {['extracting', 'running'].includes(status) ? (
        <LoaderCircle className="spin" size={10} />
      ) : (
        <Circle size={6} fill="currentColor" />
      )}
      {STATUS_LABELS[status] ?? status}
    </span>
  );
}
export function Confidence({ meta }: { meta?: FieldMeta }) {
  if (!meta) return null;
  return (
    <span
      className={`confidence ${meta.confidence < 0.85 && meta.source !== 'manual' ? 'low' : ''}`}
    >
      {meta.source === 'manual'
        ? 'Manually entered'
        : `${Math.round(meta.confidence * 100)}%${meta.confidence < 0.85 ? ' · Verify' : ''}`}
    </span>
  );
}
export function PageHeader({
  eyebrow,
  title,
  subtitle,
  actions,
}: {
  eyebrow?: string;
  title: string;
  subtitle: string;
  actions?: ReactNode;
}) {
  return (
    <div className="page-heading">
      <div>
        {eyebrow && <div className="eyebrow">{eyebrow}</div>}
        <h1>{title}</h1>
        <p>{subtitle}</p>
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </div>
  );
}
export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">
        <FileText size={26} strokeWidth={1.4} />
      </div>
      <h3>{title}</h3>
      <p>{description}</p>
      {action}
    </div>
  );
}
export function Panel({
  title,
  action,
  children,
  className = '',
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`panel ${className}`}>
      <div className="panel-heading">
        <h2>{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}
export function TextLink({ to, children }: { to: string; children: ReactNode }) {
  return (
    <Link className="text-link" to={to}>
      {children}
      <ArrowUpRight size={14} />
    </Link>
  );
}
export function Loading() {
  return (
    <div className="loading">
      <LoaderCircle className="spin" size={22} /> Loading local workspace…
    </div>
  );
}

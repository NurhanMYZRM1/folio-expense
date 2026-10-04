import {
  AnimatePresence,
  motion,
  useReducedMotion,
  useIsPresent,
  useScroll,
  useTransform,
  type Transition,
  type Variants,
} from 'motion/react';
import { useEffect, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

/**
 * Spring presets tuned to feel like UIKit/SwiftUI defaults. Every animation
 * in the app uses one of these so motion stays consistent. Only transform and
 * opacity are animated, which the compositor handles without layout or paint.
 */
export const spring = {
  /** Page and panel entrances: quick, a hint of overshoot (SwiftUI `.snappy`). */
  snappy: { type: 'spring', stiffness: 420, damping: 34, mass: 0.9 },
  /** Lists, cards, toasts: soft settle (SwiftUI `.smooth`). */
  smooth: { type: 'spring', stiffness: 300, damping: 30 },
  /** Playful pops: success checks, sheets (SwiftUI `.bouncy`). */
  bouncy: { type: 'spring', stiffness: 520, damping: 22, mass: 0.8 },
  /** Shared-element indicators (nav pill, tab underline). */
  indicator: { type: 'spring', stiffness: 560, damping: 42 },
} satisfies Record<string, Transition>;

const pageVariants: Variants = {
  initial: { opacity: 0, y: 10, scale: 0.995 },
  enter: { opacity: 1, y: 0, scale: 1, transition: spring.snappy },
  exit: { opacity: 0, y: -6, transition: { duration: 0.12, ease: [0.4, 0, 1, 1] } },
};

export function PageTransition({ children }: { children: ReactNode }) {
  return (
    <motion.div
      className="page"
      variants={pageVariants}
      initial="initial"
      animate="enter"
      exit="exit"
    >
      {children}
    </motion.div>
  );
}

/** Fades and lifts children in; `index` staggers siblings by 35ms (capped). */
export function Reveal({
  children,
  index = 0,
  className,
  as = 'div',
}: {
  children: ReactNode;
  index?: number;
  className?: string;
  as?: 'div' | 'section';
}) {
  const Component = as === 'section' ? motion.section : motion.div;
  return (
    <Component
      className={className}
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ ...spring.smooth, delay: Math.min(index, 8) * 0.035 }}
    >
      {children}
    </Component>
  );
}

/** Motion props for an item in an animated list (wrap the list in AnimatePresence). */
export function listItem(index = 0) {
  return {
    layout: 'position' as const,
    initial: { opacity: 0, y: 8 },
    animate: {
      opacity: 1,
      y: 0,
      transition: { ...spring.smooth, delay: Math.min(index, 10) * 0.025 },
    },
    exit: { opacity: 0, x: -12, transition: { duration: 0.16 } },
    transition: spring.smooth,
  };
}

/** Reveal for content inserted into the flow (forms, panels that open in place). */
export const insertMotion = {
  initial: { opacity: 0, y: -6, scale: 0.985 },
  animate: { opacity: 1, y: 0, scale: 1, transition: spring.snappy },
  exit: { opacity: 0, y: -4, scale: 0.985, transition: { duration: 0.14 } },
};

/**
 * Subtle parallax for page headers: as the page scrolls the header drifts up
 * a little faster and fades, so content appears to slide over it. Off when
 * the user prefers reduced motion.
 */
export function ParallaxHeader({
  children,
  className,
}: {
  children: ReactNode;
  className: string;
}) {
  const reduce = useReducedMotion();
  const { scrollY } = useScroll();
  const y = useTransform(scrollY, [0, 220], [0, -28], { clamp: true });
  const opacity = useTransform(scrollY, [0, 200], [1, 0.35], { clamp: true });
  return (
    <motion.div className={className} style={reduce ? undefined : { y, opacity }}>
      {children}
    </motion.div>
  );
}

/** Animated checkmark that draws itself; used for success states. */
export function SuccessCheck({ size = 18 }: { size?: number }) {
  return (
    <motion.svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      initial={{ scale: 0.6, opacity: 0 }}
      animate={{ scale: 1, opacity: 1 }}
      transition={spring.bouncy}
    >
      <circle cx="12" cy="12" r="10" opacity={0.18} fill="currentColor" stroke="none" />
      <motion.path
        d="M7.5 12.5l3 3 6-6.5"
        initial={{ pathLength: 0 }}
        animate={{ pathLength: 1 }}
        transition={{ duration: 0.32, ease: [0.65, 0, 0.35, 1], delay: 0.08 }}
      />
    </motion.svg>
  );
}

/**
 * Accessible modal: portalled, backdrop blur, spring scale-in (bottom sheet on
 * phones), focus moved inside and restored on close, Escape closes, and the
 * app behind is made inert so focus and assistive tech stay in the dialog.
 */
export function Modal({
  open,
  onClose,
  labelledBy,
  describedBy,
  role = 'dialog',
  children,
}: {
  open: boolean;
  onClose: () => void;
  labelledBy: string;
  describedBy?: string;
  role?: 'dialog' | 'alertdialog';
  children: ReactNode;
}) {
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    const shell = document.querySelector<HTMLElement>('.app-shell');
    shell?.setAttribute('inert', '');
    shell?.setAttribute('aria-hidden', 'true');
    const focusFirst = () =>
      panel.current
        ?.querySelector<HTMLElement>('[data-autofocus], button:not(:disabled), [href], input')
        ?.focus();
    const frame = requestAnimationFrame(focusFirst);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
      if (e.key === 'Tab' && panel.current) {
        const items = [
          ...panel.current.querySelectorAll<HTMLElement>(
            'button:not(:disabled), [href], input:not(:disabled), select, textarea',
          ),
        ];
        if (!items.length) return;
        const first = items[0],
          last = items[items.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    };
    document.addEventListener('keydown', onKey);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener('keydown', onKey);
      shell?.removeAttribute('inert');
      shell?.removeAttribute('aria-hidden');
      previous?.focus?.();
    };
  }, [open, onClose]);
  return createPortal(
    <AnimatePresence>
      {open && (
        <ModalLayer>
          <motion.div
            className="modal-backdrop"
            onClick={onClose}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.22 } }}
            exit={{ opacity: 0, transition: { duration: 0.18 } }}
          />
          <motion.div
            ref={panel}
            className="modal"
            role={role}
            aria-modal="true"
            aria-labelledby={labelledBy}
            aria-describedby={describedBy}
            initial={{ opacity: 0, scale: 0.94, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0, transition: spring.bouncy }}
            exit={{ opacity: 0, scale: 0.97, y: 8, transition: { duration: 0.16 } }}
          >
            {children}
          </motion.div>
        </ModalLayer>
      )}
    </AnimatePresence>,
    document.body,
  );
}

/** While exiting, the dialog layer stops intercepting input so the app is usable immediately. */
function ModalLayer({ children }: { children: ReactNode }) {
  const present = useIsPresent();
  return (
    <div
      className="modal-root"
      inert={!present}
      style={present ? undefined : { pointerEvents: 'none' }}
    >
      {children}
    </div>
  );
}

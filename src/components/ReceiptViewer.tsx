import { useEffect, useState } from 'react';
import {
  ZoomIn,
  ZoomOut,
  RotateCw,
  Maximize,
  ChevronLeft,
  ChevronRight,
  FileText,
  LoaderCircle,
} from 'lucide-react';
import { api } from '../lib/ipc';
import { decodeBase64 } from '../lib/encoding';
import { errorMessage } from '../lib/errors';
import type { PDFDocumentProxy } from 'pdfjs-dist';
export function ReceiptViewer({ receiptId }: { receiptId: string | null }) {
  const [src, setSrc] = useState(''),
    [name, setName] = useState('Receipt'),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false);
  const [zoom, setZoom] = useState(1),
    [rotation, setRotation] = useState(0),
    [page, setPage] = useState(1),
    [pdf, setPdf] = useState<PDFDocumentProxy | null>(null);
  useEffect(() => {
    let live = true,
      url = '',
      document: PDFDocumentProxy | null = null;
    setSrc('');
    setError('');
    setPage(1);
    setZoom(1);
    setRotation(0);
    setPdf(null);
    if (receiptId) {
      setBusy(true);
      void api
        .receipt(receiptId)
        .then(async (r) => {
          if (!live) return;
          setName(r.receipt.originalFilename);
          if (r.mimeType === 'application/pdf') {
            const { loadPdf } = await import('../lib/receiptRendering');
            document = await loadPdf(r);
            if (live) setPdf(document);
            else void document.loadingTask.destroy();
          } else {
            url = URL.createObjectURL(
              new Blob([decodeBase64(r.base64) as BlobPart], { type: r.mimeType }),
            );
            if (live) setSrc(url);
          }
        })
        .catch((e) => {
          if (live) setError(errorMessage(e));
        })
        .finally(() => {
          if (live) setBusy(false);
        });
    }
    return () => {
      live = false;
      if (url) URL.revokeObjectURL(url);
      if (document) void document.loadingTask.destroy();
    };
  }, [receiptId]);
  useEffect(() => {
    if (!pdf) return;
    let live = true;
    setBusy(true);
    void import('../lib/receiptRendering')
      .then(({ renderPdfPage }) => renderPdfPage(pdf, page, 1600))
      .then((canvas) => {
        if (live) setSrc(canvas.toDataURL());
        canvas.width = canvas.height = 0;
      })
      .catch((e) => {
        if (live) setError(errorMessage(e));
      })
      .finally(() => {
        if (live) setBusy(false);
      });
    return () => {
      live = false;
    };
  }, [pdf, page]);
  return (
    <section className="receipt-viewer">
      <div className="viewer-heading">
        <FileText size={15} />
        <span title={name}>{receiptId ? name : 'No receipt attached'}</span>
        <span className="local-label">LOCAL FILE</span>
      </div>
      <div className="viewer-tools">
        <div>
          <button
            className="icon-button"
            aria-label="Zoom out"
            disabled={zoom <= 0.4}
            onClick={() => setZoom((z) => z - 0.2)}
          >
            <ZoomOut size={16} />
          </button>
          <span>{Math.round(zoom * 100)}%</span>
          <button
            className="icon-button"
            aria-label="Zoom in"
            disabled={zoom >= 3}
            onClick={() => setZoom((z) => z + 0.2)}
          >
            <ZoomIn size={16} />
          </button>
        </div>
        <div>
          {pdf && (
            <>
              <button
                className="icon-button"
                aria-label="Previous receipt page"
                disabled={page === 1}
                onClick={() => setPage((p) => p - 1)}
              >
                <ChevronLeft size={16} />
              </button>
              <span>
                {page} / {pdf.numPages}
              </span>
              <button
                className="icon-button"
                aria-label="Next receipt page"
                disabled={page === pdf.numPages}
                onClick={() => setPage((p) => p + 1)}
              >
                <ChevronRight size={16} />
              </button>
            </>
          )}
          <button
            className="icon-button"
            aria-label="Rotate receipt"
            onClick={() => setRotation((r) => r + 90)}
          >
            <RotateCw size={16} />
          </button>
          <button
            className="icon-button"
            aria-label="Fit receipt to screen"
            onClick={() => {
              setZoom(1);
              setRotation(0);
            }}
          >
            <Maximize size={16} />
          </button>
        </div>
      </div>
      <div className="receipt-canvas">
        {busy && (
          <div className="viewer-loading">
            <LoaderCircle className="spin" size={22} />
            Preparing preview…
          </div>
        )}
        {error ? (
          <div className="inline-error">{error}</div>
        ) : src ? (
          <div className="receipt-image-space" style={{ width: `${zoom * 100}%` }}>
            <img
              className="receipt-image"
              src={src}
              alt={`Preview of ${name}`}
              style={{ transform: `rotate(${rotation}deg)` }}
            />
          </div>
        ) : (
          !busy && (
            <div className="empty-state">
              <FileText size={36} strokeWidth={1} />
              <h3>Manual expense</h3>
              <p>This expense does not have an attached receipt.</p>
            </div>
          )
        )}
      </div>
      <div className="viewer-footer">Original file preserved · Preview changes are not saved</div>
    </section>
  );
}

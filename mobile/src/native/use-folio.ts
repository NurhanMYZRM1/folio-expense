// Data for native screens from the Rust core, refetched whenever records change.
import { useCallback, useEffect, useRef, useState } from "react";
import { call, errorText, useDataVersion } from "~/core/folio";

export function useFolio<T>(command: string, args: Record<string, unknown> = {}) {
  const version = useDataVersion();
  const key = JSON.stringify(args);
  const [data, setData] = useState<T>();
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  const load = useCallback(async () => {
    const mine = ++seq.current;
    try {
      const result = await call<T>(command, JSON.parse(key));
      if (mine === seq.current) {
        setData(result);
        setError(null);
      }
    } catch (e) {
      if (mine === seq.current) setError(errorText(e));
    }
  }, [command, key]);

  useEffect(() => {
    void load();
  }, [load, version]);

  return { data, error, reload: load };
}

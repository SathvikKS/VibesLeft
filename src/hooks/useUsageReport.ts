import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { UsageReport } from '@/bindings/usage';

interface UseUsageReportResult {
  report: UsageReport | null;
  loading: boolean;
  error: { kind: string; message: string } | null;
  refetch: () => Promise<void>;
}

export const useUsageReport = (provider: string): UseUsageReportResult => {
  const [report, setReport] = useState<UsageReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<{ kind: string; message: string } | null>(null);

  const fetchUsage = useCallback(async (forceRefresh = false) => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<UsageReport>('get_usage_report', {
        provider,
        forceRefresh,
      });
      setReport({
        ...result,
        five_hour: { ...result.five_hour, utilization: Math.round(result.five_hour.utilization) },
        seven_day: { ...result.seven_day, utilization: Math.round(result.seven_day.utilization) },
      });
    } catch (err) {
      setError(err as { kind: string; message: string });
    } finally {
      setLoading(false);
    }
  }, [provider]);

  useEffect(() => {
    fetchUsage();
  }, [fetchUsage]);

  return { report, loading, error, refetch: () => fetchUsage(true) };
};

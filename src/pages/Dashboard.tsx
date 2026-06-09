import { useMemo, useEffect, useRef } from 'react';
import { useUsageReport } from '@/hooks/useUsageReport';
import { ENABLED_PROVIDERS } from '@/providers.config';
import { getStatusInfo, CacheIndicator } from '@/components/providers/ProviderUsageCard';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Badge } from '@/components/ui/badge';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { AlertTriangle, CheckCircle2, Info } from 'lucide-react';
import type { Page } from '@/App';

interface DashboardProps {
  setPage: (page: Page) => void;
  onUtilizationUpdate?: (utilization: Record<string, number>) => void;
}

const useProviderReport = (id: string) => {
  const { report, loading, error } = useUsageReport(id);
  return { report, loading, error, id };
};


const Sparkline = () => (
  <svg width="180" height="32" viewBox="0 0 180 32" className="overflow-visible">
    <polyline
      points="0,22 12,18 20,26 28,10 36,20 46,13 54,21 64,7 72,17 82,11 90,19 100,5 110,15 120,9 130,17 140,7 150,13 162,9 172,15 180,11"
      fill="none"
      stroke="#34d399"
      strokeWidth="1.5"
      strokeLinejoin="round"
      strokeLinecap="round"
    />
  </svg>
);

const AxisLabels = () => (
  <div className="flex justify-between text-[10px] text-muted-foreground mt-0.5 pl-0">
    {[0, 20, 40, 60, 80, 100].map((v) => (
      <span key={v}>{v}</span>
    ))}
  </div>
);

export const Dashboard = ({ setPage, onUtilizationUpdate }: DashboardProps) => {
  const results = ENABLED_PROVIDERS.map((p) => useProviderReport(p.id));

  const utilizations = useMemo(() => {
    const map: Record<string, number> = {};
    for (const r of results) {
      if (r.report) {
        const vals = [r.report.five_hour.utilization, r.report.seven_day.utilization];
        if (r.report.metadata) {
          vals.push(
            r.report.metadata.secondary_five_hour.utilization,
            r.report.metadata.secondary_seven_day.utilization,
          );
        }
        map[r.id] = Math.max(...vals);
      }
    }
    return map;
  }, [results]);

  const prevUtilRef = useRef('');
  useEffect(() => {
    const serialized = JSON.stringify(utilizations);
    if (serialized !== prevUtilRef.current) {
      prevUtilRef.current = serialized;
      onUtilizationUpdate?.(utilizations);
    }
  }, [utilizations, onUtilizationUpdate]);

  const allReports = results.filter(
    (r): r is typeof r & { report: NonNullable<typeof r.report> } => r.report !== null,
  );

  const globalMaxUtilization =
    allReports.length > 0
      ? Math.max(
          ...allReports.map((r) => {
            const vals = [r.report.five_hour.utilization, r.report.seven_day.utilization];
            if (r.report.metadata) {
              vals.push(
                r.report.metadata.secondary_five_hour.utilization,
                r.report.metadata.secondary_seven_day.utilization,
              );
            }
            return Math.max(...vals);
          }),
        )
      : 0;

  const healthCounts = useMemo(() => {
    let healthy = 0;
    let warning = 0;
    let critical = 0;
    for (const r of allReports) {
      const vals = [r.report.five_hour.utilization, r.report.seven_day.utilization];
      if (r.report.metadata) {
        vals.push(
          r.report.metadata.secondary_five_hour.utilization,
          r.report.metadata.secondary_seven_day.utilization,
        );
      }
      const maxU = Math.max(...vals);
      if (maxU >= 90) critical++;
      else if (maxU >= 75) warning++;
      else healthy++;
    }
    return { healthy, warning, critical };
  }, [allReports]);

  const overallStatus = healthCounts.critical > 0 ? 'critical' : 'healthy';
  const loadingCount = results.filter((r) => r.loading).length;
  const anyLoading = loadingCount > 0;

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground flex items-center gap-3">
            Dashboard
            {allReports.length > 0 && (
              <Badge
                variant="outline"
                className={`gap-1 leading-none ${
                  overallStatus === 'critical'
                    ? 'border-red-800/40 bg-red-950/30 text-red-300'
                    : 'border-emerald-800/40 bg-emerald-950/30 text-emerald-300'
                }`}
              >
                {overallStatus === 'critical' ? (
                  <AlertTriangle size={12} className="text-red-400" />
                ) : (
                  <CheckCircle2 size={12} className="text-emerald-400" />
                )}
                {overallStatus === 'critical'
                  ? 'Vibes off'
                  : 'All good'}
              </Badge>
            )}
          </h1>
          <p className="text-sm text-muted-foreground mt-1">Aggregate usage across all providers</p>
        </div>
      </div>

      <div className="grid grid-cols-3 gap-4 max-w-2xl mx-auto w-full">
        {(
          [
            {
              label: 'Healthy',
              value: healthCounts.healthy,
              tooltip:
                'Providers with peak utilization below 75% across all rate-limit windows — operating within safe limits.',
              valueColor: 'text-emerald-400',
              dotColor: 'bg-emerald-500',
              cardBg: 'bg-emerald-950/20',
              borderAccent: 'border-emerald-800/40',
            },
            {
              label: 'Warning',
              value: healthCounts.warning,
              tooltip:
                'Providers with peak utilization between 75–89% — approaching rate limits. Monitor closely.',
              valueColor: 'text-amber-400',
              dotColor: 'bg-amber-500',
              cardBg: 'bg-amber-950/20',
              borderAccent: 'border-amber-800/40',
            },
            {
              label: 'Critical',
              value: healthCounts.critical,
              tooltip:
                'Providers with peak utilization at or above 90% — at high risk of hitting rate limits.',
              valueColor: 'text-red-400',
              dotColor: 'bg-red-500',
              cardBg: 'bg-red-950/20',
              borderAccent: 'border-red-800/40',
            },
          ] as const
        ).map(({ label, value, tooltip, valueColor, dotColor, cardBg, borderAccent }) => (
          <div
            key={label}
            className={`flex flex-col gap-4 rounded-xl border ${borderAccent} ${cardBg} p-5`}
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <span className={`h-2 w-2 shrink-0 rounded-full ${dotColor}`} />
                <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {label}
                </span>
              </div>
              <Tooltip>
                <TooltipTrigger asChild>
                  <button className="rounded text-muted-foreground/40 outline-none transition-colors hover:text-muted-foreground/80 focus-visible:ring-1 focus-visible:ring-ring">
                    <Info size={13} />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="top" className="max-w-[200px] text-center">
                  {tooltip}
                </TooltipContent>
              </Tooltip>
            </div>
            <div>
              <p className={`text-4xl font-bold tracking-tight ${valueColor}`}>
                {anyLoading ? '—' : value}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                {anyLoading ? '' : value === 1 ? 'provider' : 'providers'}
              </p>
            </div>
          </div>
        ))}

        {/* Peak Utilization */}
        <div className="col-span-3 flex items-center justify-between rounded-xl border border-border bg-card p-5">
          <div>
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                Peak Utilization
              </span>
              <Tooltip>
                <TooltipTrigger asChild>
                  <button className="rounded text-muted-foreground/40 outline-none transition-colors hover:text-muted-foreground/80 focus-visible:ring-1 focus-visible:ring-ring">
                    <Info size={13} />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="top" className="max-w-[220px]">
                  The highest utilization across all providers and rate-limit windows (5h and 7d). Represents your most constrained resource.
                </TooltipContent>
              </Tooltip>
            </div>
            <p className="mt-2 text-4xl font-bold tracking-tight text-foreground">
              {anyLoading ? '—' : `${globalMaxUtilization}%`}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">most constrained window</p>
          </div>
          {!anyLoading && allReports.length > 0 && (
            <div className="flex items-end pb-1 opacity-60">
              <Sparkline />
            </div>
          )}
        </div>
      </div>

      <div className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold text-foreground">Providers</h2>

        {ENABLED_PROVIDERS.map((provider) => {
          const result = results.find((r) => r.id === provider.id);
          const report = result?.report;
          const utilization = utilizations[provider.id] ?? 0;
          const statusInfo = getStatusInfo(utilization);

          return (
            <div key={provider.id} className="rounded-xl border bg-card p-5 shadow-sm max-w-2xl mx-auto w-full">
              <div className="flex items-center gap-3 mb-4">
                <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-background border border-border">
                  <provider.icon size={20} className="text-foreground" />
                </div>
                <div className="flex flex-col gap-1">
                  <p className="font-semibold text-foreground leading-none">{provider.name}</p>
                  {report && (
                    <CacheIndicator fetchedAtMs={report.fetched_at_ms} cached={report.cached} />
                  )}
                </div>
              </div>

              {report ? (
                <div className="flex flex-col gap-2">
                  <div className="flex items-center gap-4">
                    <span className="w-24 shrink-0 text-sm text-muted-foreground">5h Window</span>
                    <div className="flex-1">
                      <Progress
                        value={report.five_hour.utilization}
                        indicatorClassName={getStatusInfo(report.five_hour.utilization).bg}
                        className="h-2"
                      />
                    </div>
                    <span className="w-10 text-right text-sm font-medium text-foreground">
                      {report.five_hour.utilization}%
                    </span>
                  </div>
                  <div className="flex items-center gap-4">
                    <span className="w-24 shrink-0 text-sm text-muted-foreground">7d Window</span>
                    <div className="flex-1">
                      <Progress
                        value={report.seven_day.utilization}
                        indicatorClassName={getStatusInfo(report.seven_day.utilization).bg}
                        className="h-2"
                      />
                    </div>
                    <span className="w-10 text-right text-sm font-medium text-foreground">
                      {report.seven_day.utilization}%
                    </span>
                  </div>
                  <div className="flex items-center gap-4">
                    <span className="w-24 shrink-0" />
                    <div className="flex-1">
                      <AxisLabels />
                    </div>
                    <span className="w-10" />
                  </div>
                  <div className="flex items-center justify-between mt-1">
                    <Badge
                      variant="outline"
                      className={
                        utilization >= 90
                          ? 'border-red-800/50 bg-red-950/40 text-red-400'
                          : utilization >= 75
                            ? 'border-amber-800/50 bg-amber-950/40 text-amber-400'
                            : 'border-emerald-800/50 bg-emerald-950/40 text-emerald-400'
                      }
                    >
                      {statusInfo.label}
                    </Badge>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setPage({ view: 'provider', id: provider.id })}
                    >
                      View details →
                    </Button>
                  </div>
                </div>
              ) : result?.loading ? (
                <p className="text-sm text-muted-foreground">Loading…</p>
              ) : result?.error ? (
                <p className="text-sm text-red-400">
                  {result.error.kind}: {result.error.message}
                </p>
              ) : null}
            </div>
          );
        })}
      </div>
    </div>
  );
};

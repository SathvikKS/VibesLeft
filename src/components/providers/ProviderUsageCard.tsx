import { useState, useEffect } from 'react';
import { Progress } from '@/components/ui/progress';
import { Card, CardContent } from '@/components/ui/card';
import type { UsageReport } from '@/bindings/usage';
import {
  Clock,
  CalendarDays,
  AlertTriangle,
  CheckCircle2,
  Activity,
  Hourglass,
} from 'lucide-react';

export const formatTimeUntilReset = (resetDateString: string) => {
  const resetDate = new Date(resetDateString);
  const now = new Date();
  const diffMs = resetDate.getTime() - now.getTime();

  if (diffMs <= 0) return 'Resetting now';

  const diffMins = Math.floor(diffMs / 60000);
  const days = Math.floor(diffMins / 1440);
  const hours = Math.floor((diffMins % 1440) / 60);
  const mins = diffMins % 60;

  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
};

export const getStatusInfo = (utilization: number) => {
  if (utilization >= 90)
    return {
      bg: 'bg-red-500',
      text: 'text-red-600 dark:text-red-400',
      badgeBg: 'bg-red-50 dark:bg-red-950/50',
      icon: AlertTriangle,
      label: utilization >= 100 ? 'Limit reached' : 'Critical usage',
    };
  if (utilization >= 75)
    return {
      bg: 'bg-amber-500',
      text: 'text-amber-600 dark:text-amber-400',
      badgeBg: 'bg-amber-50 dark:bg-amber-950/50',
      icon: Activity,
      label: 'Nearing limit',
    };
  return {
    bg: 'bg-emerald-500',
    text: 'text-emerald-600 dark:text-emerald-400',
    badgeBg: 'bg-emerald-50 dark:bg-emerald-950/50',
    icon: CheckCircle2,
    label: 'Within limits',
  };
};

export const CacheIndicator = ({
  fetchedAtMs,
  cached,
}: {
  fetchedAtMs: number;
  cached: boolean;
}) => {
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 10000);
    return () => clearInterval(interval);
  }, []);

  const diffMs = Math.max(0, now - fetchedAtMs);
  const diffMins = Math.floor(diffMs / 60000);

  if (!cached && diffMins < 1) {
    return (
      <div className="flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-md border border-emerald-800/50 bg-emerald-950/40 text-emerald-400">
        <span className="relative flex h-2 w-2">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" />
          <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500" />
        </span>
        Live Data
      </div>
    );
  }

  let className = 'bg-emerald-950/40 border-emerald-800/50 text-emerald-400';
  let dotClassName = 'bg-emerald-500';
  let isStale = false;

  if (diffMins >= 60) {
    className = 'bg-red-950/40 border-red-800/50 text-red-400';
    dotClassName = 'text-red-400';
    isStale = true;
  } else if (diffMins >= 15) {
    className = 'bg-amber-950/40 border-amber-800/50 text-amber-400';
    dotClassName = 'text-amber-400';
    isStale = true;
  }

  let timeText = diffMins === 0 ? 'just now' : `${diffMins}m ago`;
  if (diffMins >= 60) {
    const hours = Math.floor(diffMins / 60);
    const remainingMins = diffMins % 60;
    timeText =
      remainingMins > 0 ? `${hours}h ${remainingMins}m ago` : `${hours}h ago`;
  }

  const exactDate = new Date(fetchedAtMs).toLocaleString();

  return (
    <div
      className={`flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-md border ${className}`}
      title={`Data fetched at: ${exactDate}`}
    >
      {isStale ? (
        <AlertTriangle size={12} className={dotClassName} />
      ) : (
        <div className={`w-1.5 h-1.5 rounded-full ${dotClassName}`} />
      )}
      {cached ? `Cached ${timeText}` : `Updated ${timeText}`}
    </div>
  );
};

export const MetricRow = ({
  title,
  icon: Icon,
  data,
}: {
  title: string;
  icon: typeof Clock;
  data: { utilization: number; resets_at: string };
}) => {
  const { utilization, resets_at } = data;
  const statusInfo = getStatusInfo(utilization);
  const StatusIcon = statusInfo.icon;

  const resetText = formatTimeUntilReset(resets_at);
  const exactResetDate = new Date(resets_at).toLocaleString();

  const localResetTime = new Date(resets_at).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });

  return (
    <div className="flex flex-col gap-2.5 p-4 rounded-xl bg-card border border-border shadow-sm transition-all hover:shadow-md">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="p-1.5 rounded-md bg-muted text-muted-foreground">
            <Icon size={16} />
          </div>
          <span className="font-semibold text-foreground text-sm">{title}</span>
        </div>

        <div className="flex items-center gap-3">
          <div
            className={`flex items-center gap-1.5 px-2 py-0.5 rounded-md ${statusInfo.badgeBg} ${statusInfo.text} text-[11px] font-medium border border-transparent`}
          >
            <StatusIcon size={12} />
            {statusInfo.label}
          </div>
          <span className="font-bold text-foreground">{utilization}%</span>
        </div>
      </div>

      <Progress value={utilization} indicatorClassName={statusInfo.bg} />

      <div className="flex items-center justify-between mt-1">
        <span className="text-xs text-muted-foreground">{localResetTime}</span>
        <div
          className="flex items-center gap-1.5 text-xs text-muted-foreground"
          title={`Resets at: ${exactResetDate}`}
        >
          <Hourglass size={14} className="text-muted-foreground" />
          <span>
            Resets{' '}
            <span className="font-medium text-foreground">{resetText}</span>
          </span>
        </div>
      </div>
    </div>
  );
};

export const ProviderUsageCard = ({ report }: { report: UsageReport }) => {
  return (
    <Card className="w-full max-w-2xl mx-auto shadow-sm">
      <CardContent className="p-6 flex flex-col gap-4 bg-muted/30 rounded-xl">
        <MetricRow title="5-Hour Window" icon={Clock} data={report.five_hour} />
        <MetricRow
          title="7-Day Window"
          icon={CalendarDays}
          data={report.seven_day}
        />
      </CardContent>
    </Card>
  );
};

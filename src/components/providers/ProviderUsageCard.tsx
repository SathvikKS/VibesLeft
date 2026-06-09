import { useState, useEffect } from 'react';
import { Progress } from '@/components/ui/progress';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import type { UsageReport } from '@/bindings/usage';
import {
  Bot,
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
      text: 'text-red-600',
      badgeBg: 'bg-red-50',
      icon: AlertTriangle,
      label: utilization >= 100 ? 'Limit reached' : 'Critical usage',
    };
  if (utilization >= 75)
    return {
      bg: 'bg-amber-500',
      text: 'text-amber-600',
      badgeBg: 'bg-amber-50',
      icon: Activity,
      label: 'Nearing limit',
    };
  return {
    bg: 'bg-emerald-500',
    text: 'text-emerald-600',
    badgeBg: 'bg-emerald-50',
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

  let statusColor = 'text-slate-600';
  let bgColor = 'bg-slate-100';
  let borderColor = 'border-slate-200';
  let dotColor = 'bg-slate-400';
  let isStale = false;

  if (diffMins >= 60) {
    statusColor = 'text-red-700';
    bgColor = 'bg-red-50';
    borderColor = 'border-red-200';
    dotColor = 'text-red-500';
    isStale = true;
  } else if (diffMins >= 15) {
    statusColor = 'text-amber-700';
    bgColor = 'bg-amber-50';
    borderColor = 'border-amber-200';
    dotColor = 'text-amber-500';
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
      className={`flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-md border ${bgColor} ${borderColor} ${statusColor}`}
      title={`Data fetched at: ${exactDate}`}
    >
      {isStale ? (
        <AlertTriangle size={12} className={dotColor} />
      ) : (
        <div className={`w-1.5 h-1.5 rounded-full ${dotColor}`} />
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
    <div className="flex flex-col gap-2.5 p-4 rounded-xl bg-white border border-slate-100 shadow-sm transition-all hover:shadow-md">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="p-1.5 rounded-md bg-slate-100 text-slate-600">
            <Icon size={16} />
          </div>
          <span className="font-semibold text-slate-800 text-sm">{title}</span>
        </div>

        <div className="flex items-center gap-3">
          <div
            className={`flex items-center gap-1.5 px-2 py-0.5 rounded-md ${statusInfo.badgeBg} ${statusInfo.text} text-[11px] font-medium border border-transparent`}
          >
            <StatusIcon size={12} />
            {statusInfo.label}
          </div>
          <span className="font-bold text-slate-900">{utilization}%</span>
        </div>
      </div>

      <Progress value={utilization} indicatorClassName={statusInfo.bg} />

      <div className="flex items-center justify-between mt-1">
        <span className="text-xs text-slate-500">{localResetTime}</span>
        <div
          className="flex items-center gap-1.5 text-xs text-slate-500"
          title={`Resets at: ${exactResetDate}`}
        >
          <Hourglass size={14} className="text-slate-400" />
          <span>
            Resets{' '}
            <span className="font-medium text-slate-700">{resetText}</span>
          </span>
        </div>
      </div>
    </div>
  );
};

export const ProviderUsageCard = ({ report }: { report: UsageReport }) => {
  return (
    <Card className="w-full max-w-md shadow-sm">
      <CardHeader className="flex flex-row items-center justify-between pb-4 border-b border-slate-100">
        <div className="flex items-center gap-3">
          <div className="p-2.5 rounded-xl bg-indigo-50 text-indigo-600 ring-1 ring-indigo-100">
            <Bot size={24} />
          </div>
          <div className="flex flex-col items-start">
            <CardTitle className="text-lg capitalize flex items-center gap-2">
              {report.provider_name}
              <CacheIndicator
                fetchedAtMs={report.fetched_at_ms}
                cached={report.cached}
              />
            </CardTitle>
            <p className="text-sm text-slate-500 mt-0.5">
              API Consumption Metrics
            </p>
          </div>
        </div>
      </CardHeader>

      <CardContent className="pt-6 flex flex-col gap-4 bg-slate-50/50 rounded-b-xl">
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

interface UsageErrorProps {
  error: { kind: string; message: string };
  compact?: boolean;
}

export const UsageError = ({ error, compact }: UsageErrorProps) => {
  const isReauth = error.kind === 'ReauthRequired';
  return (
    <div
      className={`rounded-lg border text-sm ${
        compact ? 'p-2' : 'p-3'
      } ${
        isReauth
          ? 'border-amber-300 bg-amber-50 text-amber-800'
          : 'border-red-300 bg-red-50 text-red-800'
      }`}
    >
      {isReauth ? (
        <div>
          <strong>Re-authentication required</strong>
          {!compact && <p className="mt-1">{error.message}</p>}
        </div>
      ) : (
        <span>
          {error.kind}: {error.message}
        </span>
      )}
    </div>
  );
};

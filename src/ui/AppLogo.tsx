import { useId } from "react";

/** Any LM Desktop 로고 — src-tauri/icons/icon.svg와 같은 그림(연결된 노드 + 말풍선). */
export function AppLogo({
  size = 24,
  className,
}: {
  size?: number;
  className?: string;
}) {
  const gradientId = useId();
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 1024 1024"
      width={size}
      height={size}
      className={className}
      role="img"
      aria-label="Any LM Desktop"
    >
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#4A3AD0" />
          <stop offset="1" stopColor="#0E9F9A" />
        </linearGradient>
      </defs>
      <rect width="1024" height="1024" rx="225" fill={`url(#${gradientId})`} />
      <g
        stroke="#FFFFFF"
        strokeWidth="44"
        strokeLinecap="round"
        fill="none"
        opacity="0.85"
      >
        <path d="M240 250 L560 220 L800 320" />
        <path d="M240 250 L230 520" />
        <path d="M800 320 L760 480" />
      </g>
      <g fill="#FFFFFF">
        <circle cx="240" cy="250" r="84" />
        <circle cx="560" cy="220" r="84" />
        <circle cx="800" cy="320" r="84" />
        <circle cx="230" cy="520" r="84" />
      </g>
      <path
        fill="#FFFFFF"
        stroke="#2F3FB5"
        strokeWidth="28"
        strokeLinejoin="round"
        d="M410 470 H800 A110 110 0 0 1 910 580 V690 A110 110 0 0 1 800 800 H600 L450 910 L470 800 H410 A110 110 0 0 1 300 690 V580 A110 110 0 0 1 410 470 Z"
      />
      <g fill="#3A3FC4">
        <circle cx="465" cy="635" r="38" />
        <circle cx="605" cy="635" r="38" />
        <circle cx="745" cy="635" r="38" />
      </g>
    </svg>
  );
}

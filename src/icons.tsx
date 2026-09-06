type P = { className?: string };

const base = {
  viewBox: "0 0 24 24",
  width: "1.15em",
  height: "1.15em",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export const Bolt = ({ className }: P) => (
  <svg {...base} className={className} fill="currentColor" stroke="none">
    <path d="M13.5 2 4.5 13.2h5.2L9 22l9.6-11.6h-5.6z" />
  </svg>
);

export const Power = ({ className }: P) => (
  <svg {...base} className={className} strokeWidth={2}>
    <path d="M12 3.5v8.5" />
    <path d="M18.4 6.6a9 9 0 1 1-12.8 0" />
  </svg>
);

export const Home = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M3.5 10.5 12 4l8.5 6.5V20a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1z" />
  </svg>
);

export const Layers = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="m12 3 8.5 4.5L12 12 3.5 7.5z" />
    <path d="m4 12.5 8 4.3 8-4.3" />
    <path d="m4 17 8 4.3 8-4.3" />
  </svg>
);

export const Gauge = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M4 18a9 9 0 1 1 16 0" />
    <path d="m12 14 4.2-4.2" />
    <circle cx="12" cy="18" r="1.4" fill="currentColor" stroke="none" />
  </svg>
);

export const Terminal = ({ className }: P) => (
  <svg {...base} className={className}>
    <rect x="3" y="4.5" width="18" height="15" rx="2.5" />
    <path d="m7.5 10 2.6 2.2-2.6 2.2" />
    <path d="M12.8 14.6h4" />
  </svg>
);

export const Cog = ({ className }: P) => (
  <svg {...base} className={className}>
    <circle cx="12" cy="12" r="3.2" />
    <path d="M12 2.6v2.6M12 18.8v2.6M21.4 12h-2.6M5.2 12H2.6M18.6 5.4l-1.8 1.8M7.2 16.8l-1.8 1.8M18.6 18.6l-1.8-1.8M7.2 7.2 5.4 5.4" />
  </svg>
);

export const Download = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M12 3.5v11" />
    <path d="m7.5 10.2 4.5 4.3 4.5-4.3" />
    <path d="M4.5 19.5h15" />
  </svg>
);

export const Folder = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M3.5 7a2 2 0 0 1 2-2h3.6l2 2.6h7.4a2 2 0 0 1 2 2v8.4a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" />
  </svg>
);

export const Check = ({ className }: P) => (
  <svg {...base} className={className} strokeWidth={2.4}>
    <path d="m5 12.5 4.5 4.5L19 7" />
  </svg>
);

export const Cross = ({ className }: P) => (
  <svg {...base} className={className} strokeWidth={2.2}>
    <path d="M6 6l12 12M18 6 6 18" />
  </svg>
);

export const Minus = ({ className }: P) => (
  <svg {...base} className={className} strokeWidth={2}>
    <path d="M5 12h14" />
  </svg>
);

export const Square = ({ className }: P) => (
  <svg {...base} className={className} strokeWidth={1.6}>
    <rect x="6" y="6" width="12" height="12" rx="2" />
  </svg>
);

export const Refresh = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M20 11.5a8 8 0 1 0-2.4 5.7" />
    <path d="M20 4.5v5h-5" />
  </svg>
);

export const Crown = ({ className }: P) => (
  <svg {...base} className={className} fill="currentColor" stroke="none">
    <path d="M3 8.5 6.8 12l3.6-6 3.6 6L18 8.5l-1.4 9.5H4.4z" />
  </svg>
);

export const Search = ({ className }: P) => (
  <svg {...base} className={className}>
    <circle cx="11" cy="11" r="6.2" />
    <path d="m16 16 4 4" />
  </svg>
);

export const Shield = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M12 3 5 6v6.2c0 4 2.9 7.4 7 8.8 4.1-1.4 7-4.8 7-8.8V6z" />
  </svg>
);

export const Alert = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M12 8.5v5" />
    <circle cx="12" cy="17" r="1" fill="currentColor" stroke="none" />
    <path d="M10.3 3.9 2.9 17.4A2 2 0 0 0 4.6 20.4h14.8a2 2 0 0 0 1.7-3l-7.4-13.5a2 2 0 0 0-3.4 0z" />
  </svg>
);

export const Rocket = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M13.5 3.5c3.5 0 7 3.5 7 7-1.6 4.2-5 7.2-9.2 8.4L8 16.6C9.2 12.4 12 6 13.5 3.5z" />
    <circle cx="15" cy="9" r="1.6" />
    <path d="M8 16.5 4.6 19.9M5.5 14.5 3 17M9.5 19.5 7 22" />
  </svg>
);

export const Plus = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M12 5v14M5 12h14" />
  </svg>
);

export const Trash = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M4 7h16M10 7V5.2a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1V7" />
    <path d="M6.5 7 7.4 19a1 1 0 0 0 1 .9h7.2a1 1 0 0 0 1-.9L17.5 7" />
    <path d="M10.5 11v5M13.5 11v5" />
  </svg>
);

export const Pencil = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M4 20h4L19.5 8.5a2.1 2.1 0 0 0-3-3L5 17z" />
    <path d="M14.5 5.5l4 4" />
  </svg>
);

export const Globe = ({ className }: P) => (
  <svg {...base} className={className}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M3.5 12h17" />
    <path d="M12 3.5c2.2 2.4 3.4 5.4 3.4 8.5s-1.2 6.1-3.4 8.5c-2.2-2.4-3.4-5.4-3.4-8.5S9.8 5.9 12 3.5z" />
  </svg>
);

export const Bulb = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M9.2 17.5h5.6" />
    <path d="M10 20.5h4" />
    <path d="M12 3.5a6 6 0 0 0-3.5 10.9c.5.4.8 1 .8 1.6h5.4c0-.6.3-1.2.8-1.6A6 6 0 0 0 12 3.5z" />
  </svg>
);

export const Link = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M10.5 13.5a3.5 3.5 0 0 0 5 0l3-3a3.5 3.5 0 0 0-5-5l-1.6 1.6" />
    <path d="M13.5 10.5a3.5 3.5 0 0 0-5 0l-3 3a3.5 3.5 0 0 0 5 5l1.6-1.6" />
  </svg>
);

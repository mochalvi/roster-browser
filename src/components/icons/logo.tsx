export const Logo = (props: React.SVGProps<SVGSVGElement>) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width={512}
    height={512}
    role="graphics-symbol img"
    viewBox="0 0 512 512"
    {...props}
  >
    <title>Roster Browser</title>
    <defs>
      <linearGradient id="roster-logo-g" x1="0" y1="0" x2="0.8" y2="1">
        <stop offset="0" stopColor="#FF8A4C" />
        <stop offset="1" stopColor="#EE4B4B" />
      </linearGradient>
    </defs>
    <rect width="512" height="512" rx="112" fill="url(#roster-logo-g)" />
    <g
      fill="none"
      stroke="#FFFFFF"
      strokeWidth="64"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M176 390V186H270A62 62 0 0 1 270 310H176" />
      <path d="M262 310L348 390" />
    </g>
    <circle cx="188" cy="148" r="24" fill="#FFD86B" />
    <circle cx="236" cy="132" r="29" fill="#FFD86B" />
    <circle cx="284" cy="148" r="24" fill="#FFD86B" />
    <path d="M360 222L414 248L360 274Z" fill="#FFD86B" />
  </svg>
);

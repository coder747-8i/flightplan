<script lang="ts">
  // The login screen's stock picture, drawn so it ships with the app: a sky for the time of day
  // over layered ridges, a sectional-style grid, a magenta route and a light airplane on it.
  let { phase }: { phase: 'morning' | 'afternoon' | 'evening' | 'night' } = $props()

  const SKY = {
    morning: ['#9ec5ea', '#f6d3b0', '#fbe6c8'],
    afternoon: ['#4f8fd6', '#9cc6ee', '#dceaf6'],
    evening: ['#3a3f78', '#c9637a', '#f2a65e'],
    night: ['#070b1a', '#16224a', '#2b3a6b'],
  }
  const RIDGES = {
    morning: ['#b7a4a8', '#8f7f8e', '#5f5670'],
    afternoon: ['#9fb4a2', '#6f8f7a', '#46664f'],
    evening: ['#6b4a6e', '#4a3557', '#2b2140'],
    night: ['#1d2747', '#151d38', '#0c1226'],
  }
  const sky = $derived(SKY[phase])
  const ridge = $derived(RIDGES[phase])
  const sun = $derived(phase === 'morning' ? { x: 260, y: 470, r: 46, c: '#fff1d6' } : phase === 'afternoon' ? { x: 880, y: 160, r: 40, c: '#fffbea' } : phase === 'evening' ? { x: 820, y: 520, r: 60, c: '#ffd29a' } : { x: 900, y: 150, r: 26, c: '#e9eefc' })
  // a few fixed stars for the night sky
  const stars = Array.from({ length: 70 }, (_, i) => ({ x: (i * 397) % 1200, y: (i * 211) % 520, r: (i % 3) * 0.5 + 0.6, o: 0.35 + ((i * 37) % 60) / 100 }))
  const route = 'M 140 700 L 420 520 L 700 470 L 1080 250'
</script>

<svg viewBox="0 0 1200 900" preserveAspectRatio="xMidYMid slice" role="img" aria-label="A light airplane flying a magenta route over mountains">
  <defs>
    <linearGradient id="la-sky" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color={sky[0]} />
      <stop offset="0.62" stop-color={sky[1]} />
      <stop offset="1" stop-color={sky[2]} />
    </linearGradient>
    <radialGradient id="la-glow">
      <stop offset="0" stop-color={sun.c} stop-opacity="0.8" />
      <stop offset="1" stop-color={sun.c} stop-opacity="0" />
    </radialGradient>
    <pattern id="la-grid" width="150" height="150" patternUnits="userSpaceOnUse">
      <path d="M150 0H0V150" fill="none" stroke="#ffffff" stroke-opacity="0.13" stroke-width="1.2" />
    </pattern>
  </defs>

  <rect width="1200" height="900" fill="url(#la-sky)" />
  {#if phase === 'night'}
    {#each stars as s}<circle cx={s.x} cy={s.y} r={s.r} fill="#fff" opacity={s.o} />{/each}
  {/if}
  <circle cx={sun.x} cy={sun.y} r={sun.r * 4} fill="url(#la-glow)" />
  <circle cx={sun.x} cy={sun.y} r={sun.r} fill={sun.c} />
  {#if phase === 'night'}<circle cx={sun.x + 10} cy={sun.y - 6} r={sun.r} fill={sky[0]} />{/if}

  <!-- clouds -->
  <g fill="#ffffff" opacity={phase === 'night' ? 0.06 : 0.55}>
    <ellipse cx="300" cy="210" rx="150" ry="20" />
    <ellipse cx="380" cy="196" rx="90" ry="18" />
    <ellipse cx="960" cy="380" rx="170" ry="18" />
    <ellipse cx="880" cy="368" rx="80" ry="15" />
  </g>

  <!-- ridges, far to near -->
  <path d="M0 640 L120 590 L230 615 L360 540 L470 590 L590 520 L720 585 L850 530 L990 600 L1100 560 L1200 590 V900 H0Z" fill={ridge[0]} />
  <path d="M0 720 L140 650 L260 690 L400 620 L540 700 L660 640 L800 705 L960 630 L1080 690 L1200 650 V900 H0Z" fill={ridge[1]} />
  <path d="M0 800 L160 735 L300 775 L470 720 L620 790 L780 740 L930 800 L1090 745 L1200 775 V900 H0Z" fill={ridge[2]} />

  <rect width="1200" height="900" fill="url(#la-grid)" />

  <!-- the route -->
  <path d={route} fill="none" stroke="#ffffff" stroke-opacity="0.7" stroke-width="9" stroke-linecap="round" stroke-linejoin="round" />
  <path d={route} fill="none" stroke="#d4148c" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round" />
  {#each [[140, 700], [420, 520], [700, 470]] as [x, y]}
    <circle cx={x} cy={y} r="9" fill="#ffffff" stroke="#d4148c" stroke-width="4" />
  {/each}

  <!-- a high-wing single, seen from above, heading up the last leg (with its shadow) -->
  <g transform="translate(890 360) rotate(60) scale(2.1)">
    <path transform="translate(14 22)" d={PLANE} fill="#000" opacity="0.18" />
    <path d={PLANE} fill="#ffffff" stroke="#1b1f26" stroke-opacity="0.35" stroke-width="0.8" />
    <rect x="-31" y="-7.5" width="6" height="2" fill="#d4148c" />
    <rect x="25" y="-7.5" width="6" height="2" fill="#d4148c" />
  </g>
</svg>

<script lang="ts" module>
  const PLANE =
    'M0,-30 C3,-30 3.5,-26 3.5,-20 L3.5,-9 L31,-7.5 L31,-1 L3.5,0.5 L2.4,19 L11,20.5 L11,24.5 L1.5,25 L0,28 L-1.5,25 L-11,24.5 L-11,20.5 L-2.4,19 L-3.5,0.5 L-31,-1 L-31,-7.5 L-3.5,-9 L-3.5,-20 C-3.5,-26 -3,-30 0,-30 Z'
</script>

<style>
  svg {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>

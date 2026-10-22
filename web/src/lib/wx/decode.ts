// Plain-English decoding of METAR, TAF, PIREP, AIRMET/SIGMET text and FAA airport remarks.
// Coded groups are parsed; free text is expanded with the FAA contraction list (JO 7340).

// ---------------------------------------------------------------- contractions
const C: Record<string, string> = {
  ABV: 'above', ACFT: 'aircraft', ACR: 'air carrier', ACT: 'active', ACTVT: 'activate', ADJ: 'adjacent', ADZ: 'advise', AFT: 'after',
  AGL: 'AGL', ALF: 'aloft', ALG: 'along', ALQDS: 'all quadrants', ALSTG: 'altimeter setting', ALT: 'altitude', ALTN: 'alternate',
  AMD: 'amended', AMGR: 'airport manager', APCH: 'approach', APRX: 'approximately', APVD: 'approved', ARPT: 'airport', ARR: 'arrival',
  ASGN: 'assigned', ATA: 'airport traffic area', ATCT: 'control tower', AVBL: 'available', AVN: 'aviation', AWY: 'airway',
  BCMG: 'becoming', BFR: 'before', BKN: 'broken', BLDG: 'building', BLO: 'below', BLW: 'below', BR: 'mist', BTN: 'between', BTW: 'between',
  BYD: 'beyond', CCLKWS: 'counterclockwise', CD: 'clearance delivery', CHG: 'change', CIG: 'ceiling', CIGS: 'ceilings', CLD: 'cloud',
  CLDS: 'clouds', CLKWS: 'clockwise', CLR: 'clear', CLSD: 'closed', CNTR: 'center', CNTRLN: 'centerline', CONDS: 'conditions',
  CONS: 'continuous', CONSLY: 'continuously', CONT: 'continue', CONTG: 'continuing', CONTRAILS: 'contrails', CTC: 'contact', CTN: 'caution',
  DALGT: 'daylight', DCR: 'decrease', DCRG: 'decreasing', DEP: 'departure', DIAM: 'diameter', DLY: 'daily', DMSHG: 'diminishing',
  DSIPT: 'dissipate', DSIPTG: 'dissipating', DSND: 'descend', DSNT: 'distant', DSPLCD: 'displaced', DTHR: 'displaced threshold',
  DURG: 'during', DVLP: 'develop', DVLPG: 'developing', E: 'east', EFCT: 'effect', EMBD: 'embedded', ENTR: 'entire', EQPT: 'equipment',
  EXC: 'except', EXTRM: 'extreme', FL: 'FL', FLD: 'field', FM: 'from', FQT: 'frequent', FREQ: 'frequency', FRM: 'from', FRQ: 'frequent',
  FT: 'ft', FZLVL: 'freezing level', FZRA: 'freezing rain', GEN: 'general', GRDL: 'gradual', GRDLY: 'gradually', HDG: 'heading',
  HEL: 'helicopter', HLCPTR: 'helicopter', HOL: 'holidays', HR: 'hour', HRS: 'hours', HVY: 'heavy', ICG: 'icing', ICGIC: 'icing in clouds',
  ICGICIP: 'icing in clouds and precipitation', IMPR: 'improve', IMPRG: 'improving', INCR: 'increase', INCRG: 'increasing', INDEFLY: 'indefinitely',
  INFO: 'information', INTSF: 'intensify', INTSFYG: 'intensifying', INTST: 'intensity', INTSTY: 'intensity', INVOF: 'in the vicinity of',
  ISOL: 'isolated', KT: 'kt', KTS: 'kt', LCL: 'local', LGT: 'light', LGTD: 'lighted', LGTS: 'lights', LIRL: 'low-intensity runway lights',
  LLWS: 'low-level wind shear', LN: 'line', LNDG: 'landing', LRG: 'large', LTD: 'limited', LTG: 'lightning', LVL: 'level', LYR: 'layer',
  LYRS: 'layers', MAINT: 'maintenance', MED: 'medium', MIL: 'military', MIRL: 'medium-intensity runway lights', MOD: 'moderate',
  MOV: 'moving', MOVG: 'moving', MSL: 'MSL', MT: 'mountain', MTN: 'mountain', MTNS: 'mountains', MXD: 'mixed', N: 'north', NA: 'not authorized',
  NC: 'no change', NE: 'northeast', NML: 'normal', NMRS: 'numerous', NR: 'number', NW: 'northwest', OBSC: 'obscured', OBSCD: 'obscured',
  OBSCG: 'obscuring', OBSTN: 'obstruction', OCNL: 'occasional', OCNLY: 'occasionally', OPNS: 'operations', OPR: 'operate', OPS: 'operations',
  OTLK: 'outlook', OTR: 'other', OTRW: 'otherwise', OVC: 'overcast', OVNGT: 'overnight', PAT: 'pattern', PCPN: 'precipitation', PPR: 'prior permission required',
  PRKG: 'parking', PROC: 'procedure', PRVDD: 'provided', PSG: 'passing', PSGR: 'passenger', PTNS: 'portions', PWR: 'power', QSTNRY: 'quasi-stationary',
  RA: 'rain', RDC: 'runway design code', RGN: 'region', RMN: 'remain', RQR: 'required', RQRD: 'required', RSTD: 'restricted', RUF: 'rough',
  RWY: 'runway', RWYS: 'runways', S: 'south', SCT: 'scattered', SE: 'southeast', SEV: 'severe', SFC: 'surface', SHRA: 'rain showers',
  SKED: 'scheduled', SLO: 'slow', SN: 'snow', SR: 'sunrise', SS: 'sunset', STG: 'strong', STNR: 'stationary', STWY: 'stopway', SUSTAINED: 'sustained',
  SVC: 'service', SVCS: 'services', SW: 'southwest', SXN: 'section', TFC: 'traffic', TGL: 'touch-and-go landings', THR: 'threshold', THRU: 'through',
  TKOF: 'takeoff', TOP: 'top', TOPS: 'tops', TPA: 'pattern altitude', TRAN: 'transient', TS: 'thunderstorms', TSNT: 'transient', TSTM: 'thunderstorm',
  TSTMS: 'thunderstorms', TURB: 'turbulence', TWR: 'tower', TWY: 'taxiway', TWYS: 'taxiways', UNKN: 'unknown', UNLGT: 'unlighted', UNLGTD: 'unlighted',
  UNMKD: 'unmarked', UNSKED: 'unscheduled', UNRELBL: 'unreliable', UPDFTS: 'updrafts', VC: 'vicinity', VIS: 'visibility', VRB: 'variable',
  VRBL: 'variable', W: 'west', WDLY: 'widely', WDSPRD: 'widespread', WI: 'within', WIND: 'wind', WINDS: 'winds', WK: 'weak', WKN: 'weaken',
  WKNG: 'weakening', WKDAYS: 'weekdays', WKENDS: 'weekends', WKNDS: 'weekends', WND: 'wind', WRMFNT: 'warm front', WS: 'wind shear', WSHFT: 'wind shift',
  WSPN: 'wingspan', WTR: 'water', WV: 'wave', WX: 'weather', XCP: 'except', AFT_HRS: 'after hours', CC: 'credit card', OFC: 'office',
  OPRS: 'operators', OPRG: 'operating', WGT: 'weight', WT: 'weight', CERT: 'certified', HAZ: 'hazard', PJE: 'parachute jumping',
  REIL: 'runway end identifier lights', HIRL: 'high-intensity runway lights', BCN: 'beacon', RON: 'remain overnight', INBD: 'inbound',
  OUTBD: 'outbound', DRCT: 'direct', POSS: 'possible', SVRL: 'several', AFT_: 'after', OBS: 'observed', FCST: 'forecast', DMSH: 'diminish', SKC: 'sky clear', DSPLCD_THR: 'displaced threshold', CTAF_: 'CTAF',
}
// left as-is: identifiers and terms pilots know (also anything containing digits)
const KEEP = new Set(['CTAF', 'UNICOM', 'PAPI', 'VASI', 'ILS', 'VOR', 'VORTAC', 'NDB', 'DME', 'GPS', 'RNAV', 'ATIS', 'ASOS', 'AWOS', 'ATC', 'FAA', 'FSS',
  'IFR', 'VFR', 'MVFR', 'LIFR', 'NOTAM', 'NOTAMS', 'TFR', 'MOA', 'UAS', 'ADS', 'ARFF', 'PCL', 'AIRMET', 'SIGMET', 'PIREP', 'CB', 'TCU', 'CAT', 'UTC', 'MON',
  'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN', 'JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC', 'II', 'III', 'NM', 'SM', 'CBP', 'DOD'])
const ENGLISH = new Set(['FROM', 'HAIL', 'TORNADOES', 'GUSTS', 'SEVERE', 'AREA', 'LINE', 'VALID', 'A', 'AN', 'AND', 'ARE', 'AS', 'AT', 'BE', 'BUT', 'BY', 'FOR', 'IF', 'IN', 'IS', 'IT', 'NO', 'NOT', 'OF', 'ON', 'OR', 'THE', 'TO', 'UP',
  'WITH', 'ALL', 'ANY', 'USE', 'FLY', 'WHEN', 'THEN', 'ONLY', 'MUST', 'MAY', 'CALL', 'AREA', 'AREAS', 'ENTER', 'ALSO', 'OVER', 'UNDER', 'INTO', 'OUT',
  'NEAR', 'END', 'ENDS', 'EAST', 'WEST', 'NORTH', 'SOUTH', 'LEFT', 'RIGHT', 'STRAIGHT', 'AHEAD', 'PRIOR', 'UNTIL', 'UNLESS', 'OTHER', 'TIMES', 'BOTH',
  'SIDE', 'FIELD', 'FUEL', 'SELF', 'SERVE', 'CREDIT', 'CARD', 'HANGAR', 'RAMP', 'APRON', 'GRASS', 'TURF', 'GRAVEL', 'DIRT', 'WATER', 'BIRDS', 'DEER',
  'WILDLIFE', 'TREES', 'HILL', 'HILLS', 'TOWER', 'TOWERS', 'POLE', 'POLES', 'LINES', 'LINE', 'ROAD', 'HWY', 'FENCE', 'BLAST', 'PAD', 'NOISE', 'ABATEMENT',
  'CURFEW', 'VOLUNTARY', 'SENSITIVE', 'QUADS', 'HELICOPTERS', 'JETS', 'JET', 'PROP', 'LIGHT', 'HEAVY', 'TURBO', 'NON', 'LARGE', 'SMALL', 'FEE', 'FEES',
  'CUSTOMS', 'USER', 'PREPAID', 'ONLINE', 'SERVICE', 'AVAILABLE', 'CLOSED', 'OPEN', 'OPENS', 'DAYS', 'DAY', 'NIGHT', 'NIGHTS', 'WEEK', 'MONTH', 'YEAR',
  'BASED', 'PARKING', 'RESTRICTED', 'PRIVATE', 'PUBLIC', 'PERMISSION', 'REQUIRED', 'REQUEST', 'CONTACT', 'PHONE', 'RADIO', 'FREQUENCY', 'MILES', 'MILE',
  'FEET', 'INCHES', 'LB', 'LBS', 'MAX', 'MIN', 'MAXIMUM', 'MINIMUM', 'GREATER', 'LESS', 'THAN', 'TIP', 'BEYOND', 'MIDFIELD', 'STOP', 'GO', 'CALM', 'WIND',
  'SLOPE', 'SPEED', 'SOFT', 'WET', 'ICE', 'SNOW', 'RAIN', 'FOG', 'SMOKE', 'DUST', 'HAZE', 'MIST', 'MAINTAIN', 'CLIMB', 'DESCEND', 'TURN', 'CROSSWIND',
  'DOWNWIND', 'BASE', 'FINAL', 'PATTERN', 'SIMILAR', 'ALIGNED', 'JOINT', 'CIVIL', 'MILITARY', 'TRAINING', 'TRAFFIC', 'ATTENDED', 'UNATTENDED', 'IRREGULAR',
  'AIRPORT', 'AIRCRAFT', 'OWNER', 'MANAGER', 'OFFICE', 'AFTER', 'HOURS', 'SUNRISE', 'SUNSET', 'STATION', 'ONE', 'TWO', 'THREE', 'FOUR', 'FIVE',
  'ESTABLISHED', 'REIL', 'DEPARTURE', 'ARRIVAL', 'ARRIVALS', 'DEPARTURES', 'TAXI', 'TAXIING', 'RUNUP', 'RUNUPS', 'ENGINE', 'TEST', 'TESTS', 'PROHIBITED'])

/** Expand FAA contractions in free text and soften the all-caps. */
export function expand(text: string): string {
  if (!text) return ''
  let out = text.replace(/\bAFT HRS\b/g, 'after hours').replace(/\bSELF[- ]SVC\b/g, 'self-service').replace(/\bH24\b/g, '24 hours')
  out = out.replace(/\b[A-Z][A-Z+]*\b/g, (w) => {
    if (C[w]) return C[w]
    if (KEEP.has(w)) return w
    if (ENGLISH.has(w) || w.length > 4) return w.toLowerCase()
    return w // short unknown words are usually identifiers
  })
  out = out.replace(/\s+/g, ' ').replace(/\s+([.,;])/g, '$1').trim()
  return out.charAt(0).toUpperCase() + out.slice(1)
}

// ---------------------------------------------------------------- METAR / TAF groups
const INTENSITY: Record<string, string> = { '-': 'light ', '+': 'heavy ', VC: 'nearby ' }
const DESCRIPTOR: Record<string, string> = { MI: 'shallow ', PR: 'partial ', BC: 'patches of ', DR: 'low drifting ', BL: 'blowing ', SH: 'showers of ', TS: 'thunderstorm with ', FZ: 'freezing ' }
const PHENOM: Record<string, string> = {
  DZ: 'drizzle', RA: 'rain', SN: 'snow', SG: 'snow grains', IC: 'ice crystals', PL: 'ice pellets', GR: 'hail', GS: 'small hail', UP: 'unknown precipitation',
  BR: 'mist', FG: 'fog', FU: 'smoke', VA: 'volcanic ash', DU: 'dust', SA: 'sand', HZ: 'haze', PY: 'spray', PO: 'dust whirls', SQ: 'squalls', FC: 'funnel cloud',
  SS: 'sandstorm', DS: 'duststorm',
}
const COVER: Record<string, string> = { FEW: 'few clouds', SCT: 'scattered clouds', BKN: 'broken clouds', OVC: 'overcast', VV: 'sky obscured, vertical visibility' }

const ft = (hundreds: string) => `${(parseInt(hundreds, 10) * 100).toLocaleString()} ft`

function wind(tok: string): string | null {
  const m = tok.match(/^(\d{3}|VRB)(\d{2,3})(?:G(\d{2,3}))?(KT|MPS)$/)
  if (!m) return null
  const unit = m[4] === 'MPS' ? 'm/s' : 'kt'
  if (m[1] === '000' && m[2] === '00') return 'Wind calm'
  const dir = m[1] === 'VRB' ? 'variable direction' : `from ${m[1]}°`
  return `Wind ${dir} at ${parseInt(m[2], 10)} ${unit}${m[3] ? `, gusting ${parseInt(m[3], 10)} ${unit}` : ''}`
}

function visibility(tok: string, prev?: string): string | null {
  if (tok === 'P6SM') return 'Visibility more than 6 miles'
  if (tok === 'CAVOK') return 'Ceiling and visibility OK'
  let m = tok.match(/^(M)?(\d+)?(?:\/(\d+))?SM$/)
  if (m) {
    let v = m[3] ? parseInt(m[2], 10) / parseInt(m[3], 10) : parseInt(m[2], 10)
    if (prev && /^\d$/.test(prev) && m[3]) v += parseInt(prev, 10)
    const txt = Number.isInteger(v) ? String(v) : v < 1 ? `${m[2]}/${m[3]}` : `${Math.floor(v)} ${m[2]}/${m[3]}`
    return `Visibility ${m[1] ? 'less than ' : ''}${txt} mile${v === 1 ? '' : 's'}`
  }
  m = tok.match(/^(\d{4})$/)
  if (m) {
    if (m[1] === '9999') return 'Visibility 6 miles or more'
    const sm = parseInt(m[1], 10) / 1609.34
    return `Visibility ${sm < 3 ? sm.toFixed(1) : Math.round(sm)} miles`
  }
  return null
}

function weather(tok: string): string | null {
  const m = tok.match(/^(-|\+|VC)?(MI|PR|BC|DR|BL|SH|TS|FZ)?((?:DZ|RA|SN|SG|IC|PL|GR|GS|UP|BR|FG|FU|VA|DU|SA|HZ|PY|PO|SQ|FC|SS|DS)+)?$/)
  if (!m || (!m[2] && !m[3])) return null
  const ph = (m[3] ?? '').match(/../g)?.map((p) => PHENOM[p]).join(' and ') ?? ''
  if (m[2] === 'TS' && !ph) return `${m[1] === 'VC' ? 'Thunderstorm nearby' : 'Thunderstorm'}`
  if (m[2] === 'SH' && !ph) return m[1] === 'VC' ? 'Showers nearby' : 'Showers'
  if (m[2] === 'TS') return `Thunderstorm with ${INTENSITY[m[1] ?? ''] && m[1] !== 'VC' ? INTENSITY[m[1]!] : ''}${ph}${m[1] === 'VC' ? ' nearby' : ''}`
  let s = `${INTENSITY[m[1] ?? ''] ?? ''}${DESCRIPTOR[m[2] ?? ''] ?? ''}${ph}`.trim()
  if (m[1] === 'VC') s = `${s.replace('nearby ', '')} nearby`
  return s.charAt(0).toUpperCase() + s.slice(1)
}

function sky(tok: string): string | null {
  if (/^(CLR|SKC|NSC|NCD)$/.test(tok)) return 'Sky clear'
  const m = tok.match(/^(FEW|SCT|BKN|OVC|VV)(\d{3}|\/\/\/)(CB|TCU)?$/)
  if (!m) return null
  const base = m[2] === '///' ? 'unknown height' : ft(m[2])
  return `${COVER[m[1]].charAt(0).toUpperCase() + COVER[m[1]].slice(1)} ${m[1] === 'VV' ? base : `at ${base}`}${m[3] === 'CB' ? ' (cumulonimbus)' : m[3] === 'TCU' ? ' (towering cumulus)' : ''}`
}

const tempC = (s: string) => (s.startsWith('M') ? -parseInt(s.slice(1), 10) : parseInt(s, 10))
// output units: °F and inches of mercury
const degF = (c: number, digits = 0) => `${(c * 9 / 5 + 32).toFixed(digits)}°F`
const inHg = (hpa: number) => `${(hpa * 0.02953).toFixed(2)} inHg`

function common(tok: string, prev?: string): string | null {
  return wind(tok) ?? visibility(tok, prev) ?? sky(tok) ?? weather(tok)
}

function remarks(tokens: string[]): string[] {
  const out: string[] = []
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i]
    let m: RegExpMatchArray | null
    if (t === 'AO1') out.push('Automated station without precipitation sensor')
    else if (t === 'AO2') out.push('Automated station with precipitation sensor')
    else if ((m = t.match(/^SLP(\d{3})$/))) {
      const v = parseInt(m[1], 10) / 10
      out.push(`Sea-level pressure ${inHg(v < 50 ? 1000 + v : 900 + v)}`)
    } else if ((m = t.match(/^T([01])(\d{3})([01])(\d{3})$/))) {
      const a = (m[1] === '1' ? -1 : 1) * parseInt(m[2], 10) / 10
      const b = (m[3] === '1' ? -1 : 1) * parseInt(m[4], 10) / 10
      out.push(`Exact temperature ${degF(a, 1)}, dew point ${degF(b, 1)}`)
    } else if (t === 'PK' && tokens[i + 1] === 'WND' && (m = (tokens[i + 2] ?? '').match(/^(\d{3})(\d{2,3})\/(\d{2,4})$/))) {
      out.push(`Peak wind ${m[1]}° at ${parseInt(m[2], 10)} kt at ${m[3].length > 2 ? m[3].slice(0, 2) + ':' + m[3].slice(2) : ':' + m[3]}`)
      i += 2
    } else if (t === 'WSHFT') {
      out.push(`Wind shift at ${tokens[i + 1] ?? ''}${tokens[i + 2] === 'FROPA' ? ' (frontal passage)' : ''}`)
      i += tokens[i + 2] === 'FROPA' ? 2 : 1
    } else if (t === 'PRESRR') out.push('Pressure rising rapidly')
    else if (t === 'PRESFR') out.push('Pressure falling rapidly')
    else if (t === 'TSNO') out.push('Thunderstorm sensor not working')
    else if (t === 'PWINO') out.push('Precipitation identifier not working')
    else if (t === 'FZRANO') out.push('Freezing rain sensor not working')
    else if (t === 'PNO') out.push('Rain gauge not working')
    else if (t === 'RVRNO') out.push('Runway visual range not available')
    else if (t === '$') out.push('Station needs maintenance')
    else if ((m = t.match(/^LTG(IC|CC|CG|CA)*$/))) out.push(`Lightning${tokens[i + 1] && /^(DSNT|VC|OHD|ALQDS|[NESW]+)/.test(tokens[i + 1]) ? ' ' + expand(tokens[++i]).toLowerCase() : ''}`)
    else if ((m = t.match(/^([A-Z]{2})B(\d{2,4})(?:E(\d{2,4}))?$/)) && (PHENOM[m[1]] || m[1] === 'TS')) out.push(`${m[1] === 'TS' ? 'Thunderstorm' : PHENOM[m[1]].charAt(0).toUpperCase() + PHENOM[m[1]].slice(1)} began :${m[2].slice(-2)}${m[3] ? `, ended :${m[3].slice(-2)}` : ''}`)
    else if ((m = t.match(/^P(\d{4})$/))) out.push(`Precipitation last hour ${(parseInt(m[1], 10) / 100).toFixed(2)} in`)
    else if ((m = t.match(/^([67])(\d{4})$/))) out.push(`Precipitation ${m[1] === '6' ? 'last 3–6 hours' : 'last 24 hours'} ${(parseInt(m[2], 10) / 100).toFixed(2)} in`)
    else if ((m = t.match(/^4\/(\d{3})$/))) out.push(`Snow depth ${parseInt(m[1], 10)} in`)
    else if ((m = t.match(/^([12])([01])(\d{3})$/))) out.push(`${m[1] === '1' ? 'Highest' : 'Lowest'} temperature last 6 h ${degF((m[2] === '1' ? -1 : 1) * parseInt(m[3], 10) / 10)}`)
    else if ((m = t.match(/^5(\d)(\d{3})$/))) out.push(`Pressure ${['rising', 'rising', 'rising', 'rising', 'steady', 'falling', 'falling', 'falling', 'falling', 'steady'][+m[1]] ?? 'changed'} ${(parseInt(m[2], 10) / 10 * 0.02953).toFixed(2)} inHg in the last 3 h`)
    else if ((m = t.match(/^4([01])(\d{3})([01])(\d{3})$/))) out.push(`24-hour high ${degF((m[1] === '1' ? -1 : 1) * parseInt(m[2], 10) / 10)}, low ${degF((m[3] === '1' ? -1 : 1) * parseInt(m[4], 10) / 10)}`)
    else if (t === 'CIG' && tokens[i + 1]) {
      const v = tokens[++i].match(/^(\d{3})V(\d{3})$/)
      out.push(v ? `Ceiling varying ${ft(v[1])} to ${ft(v[2])}` : `Ceiling ${tokens[i]}`)
    } else if (t === 'VIS' && tokens[i + 1]) out.push(`Visibility ${tokens[++i].replace('V', ' to ')} miles`)
    else if (t === 'ACSL') out.push('Altocumulus standing lenticular clouds')
    else if (t === 'VIRGA') out.push('Virga (rain not reaching the ground)')
    else if (t === 'FROPA') out.push('Frontal passage')
    else out.push(expand(t))
  }
  // merge leftover single words into one line so it reads naturally
  return out.reduce<string[]>((acc, s) => {
    if (s.split(' ').length === 1 && acc.length && acc[acc.length - 1].split(' ').length < 6 && !/[0-9]/.test(s)) acc[acc.length - 1] += ` ${s.toLowerCase()}`
    else acc.push(s)
    return acc
  }, [])
}

/** METAR / SPECI → plain-English lines */
export function decodeMetar(raw: string): string[] {
  if (!raw) return []
  const [body, rmk] = raw.split(/\sRMK\s/)
  const toks = body.trim().split(/\s+/)
  const out: string[] = []
  let i = 0
  if (/^(METAR|SPECI)$/.test(toks[i])) {
    if (toks[i] === 'SPECI') out.push('Special (unscheduled) report')
    i++
  }
  const station = toks[i++]
  const tm = toks[i]?.match(/^(\d{2})(\d{2})(\d{2})Z$/)
  if (tm) {
    out.push(`${station}, observed day ${tm[1]} at ${tm[2]}:${tm[3]} UTC`)
    i++
  }
  for (; i < toks.length; i++) {
    const t = toks[i]
    if (t === 'AUTO') out.push('Fully automated report')
    else if (t === 'COR') out.push('Corrected report')
    else if (/^\d{3}V\d{3}$/.test(t)) out.push(`Wind direction varying ${t.slice(0, 3)}° to ${t.slice(4)}°`)
    else if (/^R\d{2}[LRC]?\//.test(t)) {
      const m = t.match(/^R(\d{2}[LRC]?)\/([PM])?(\d{4})(?:V([PM])?(\d{4}))?FT/)
      out.push(m ? `Runway ${m[1]} visual range ${m[2] === 'M' ? 'below ' : m[2] === 'P' ? 'above ' : ''}${parseInt(m[3], 10).toLocaleString()}${m[5] ? `–${parseInt(m[5], 10).toLocaleString()}` : ''} ft` : t)
    } else if (/^M?\d{2}\/(M?\d{2})?$/.test(t)) {
      const [a, b] = t.split('/')
      out.push(`Temperature ${degF(tempC(a))}${b ? `, dew point ${degF(tempC(b))} (spread ${Math.round((tempC(a) - tempC(b)) * 9 / 5)}°F)` : ''}`)
    } else if (/^A\d{4}$/.test(t)) out.push(`Altimeter ${t.slice(1, 3)}.${t.slice(3)} inHg`)
    else if (/^Q\d{4}$/.test(t)) out.push(`Altimeter ${inHg(parseInt(t.slice(1), 10))}`)
    else if (/^\d$/.test(t) && /SM$/.test(toks[i + 1] ?? '')) continue // whole part of "1 1/2SM"
    else {
      const c = common(t, toks[i - 1])
      out.push(c ?? t)
    }
  }
  if (rmk) out.push(...remarks(rmk.trim().split(/\s+/)).map((s) => `Remark: ${s}`))
  return out
}

/** TAF → periods with plain-English lines */
export function decodeTaf(raw: string): { title: string; lines: string[] }[] {
  if (!raw) return []
  const toks = raw.replace(/^TAF\s+(AMD|COR)?\s*/, (m) => (m.includes('AMD') ? 'AMD ' : '')).trim().split(/\s+/)
  const periods: { title: string; lines: string[] }[] = []
  let cur: { title: string; lines: string[] } | null = null
  let i = 0
  const amended = toks[0] === 'AMD'
  if (amended) i++
  const station = toks[i++]
  if (/^\d{6}Z$/.test(toks[i])) i++ // issue time
  const vt = toks[i]?.match(/^(\d{2})(\d{2})\/(\d{2})(\d{2})$/)
  const header = `${station}${amended ? ' (amended)' : ''}`
  if (vt) {
    cur = { title: `${header}: day ${vt[1]} ${vt[2]}:00 to day ${vt[3]} ${vt[4]}:00 UTC`, lines: [] }
    periods.push(cur)
    i++
  }
  const span = (s: string) => {
    const m = s.match(/^(\d{2})(\d{2})\/(\d{2})(\d{2})$/)
    return m ? `day ${m[1]} ${m[2]}:00 to day ${m[3]} ${m[4]}:00 UTC` : s
  }
  for (; i < toks.length; i++) {
    const t = toks[i]
    let m: RegExpMatchArray | null
    if ((m = t.match(/^FM(\d{2})(\d{2})(\d{2})$/))) periods.push((cur = { title: `From day ${m[1]} at ${m[2]}:${m[3]} UTC`, lines: [] }))
    else if (t === 'BECMG') periods.push((cur = { title: `Becoming, ${span(toks[++i])}`, lines: [] }))
    else if (t === 'TEMPO') periods.push((cur = { title: `Temporarily, ${span(toks[++i])}`, lines: [] }))
    else if ((m = t.match(/^PROB(\d{2})$/))) {
      const tempo = toks[i + 1] === 'TEMPO'
      if (tempo) i++
      periods.push((cur = { title: `${m[1]}% chance${tempo ? ' (temporary)' : ''}, ${span(toks[++i])}`, lines: [] }))
    } else if (cur) {
      if (/^\d$/.test(t) && /SM$/.test(toks[i + 1] ?? '')) continue
      if (/^WS\d{3}\/\d{5}KT$/.test(t)) cur.lines.push(`Low-level wind shear at ${ft(t.slice(2, 5))}: wind ${t.slice(6, 9)}° at ${parseInt(t.slice(9, 11), 10)} kt`)
      else if (/^QNH\d{4}INS$/.test(t)) cur.lines.push(`Lowest altimeter ${t.slice(3, 5)}.${t.slice(5, 7)} inHg`)
      else if (/^5\d{5}$/.test(t)) cur.lines.push('Turbulence forecast (military code)')
      else if (/^6\d{5}$/.test(t)) cur.lines.push('Icing forecast (military code)')
      else if (t === 'NSW') cur.lines.push('No significant weather')
      else if (/^T[XN]M?\d{2}\/\d{4}Z$/.test(t)) {
        const mm = t.match(/^T([XN])(M?\d{2})\/(\d{2})(\d{2})Z$/)!
        cur.lines.push(`${mm[1] === 'X' ? 'High' : 'Low'} temperature ${degF(tempC(mm[2]))} at ${mm[4]}:00 UTC on day ${mm[3]}`)
      } else if (/^(AUTOMATED|SENSOR|METWATCH|LAST|NO|AMDS|AFT|TIL|FROM|AND)$/.test(t)) cur.lines.push(expand(toks.slice(i).join(' ')))
      else cur.lines.push(common(t, toks[i - 1]) ?? expand(t))
      if (/^(AUTOMATED|SENSOR|METWATCH|LAST|NO|AMDS|AFT|TIL|FROM|AND)$/.test(t)) break
    }
  }
  return periods
}

// ---------------------------------------------------------------- PIREP
const TURB: Record<string, string> = { NEG: 'no', SMTH: 'smooth', LGT: 'light', 'SMTH-LGT': 'smooth to light', 'LGT-MOD': 'light to moderate', MOD: 'moderate', 'MOD-SEV': 'moderate to severe', SEV: 'severe', EXTRM: 'extreme', OCNL: 'occasional', INTMT: 'intermittent', CONS: 'continuous', CHOP: 'chop', CAT: 'clear-air turbulence', LLWS: 'low-level wind shear', TRACE: 'trace', TR: 'trace', CLR: 'clear', RIME: 'rime', MX: 'mixed', MXD: 'mixed' }

/** PIREP (UA/UUA) → plain-English lines */
export function decodePirep(raw: string): string[] {
  if (!raw) return []
  const parts = raw.split('/').map((s) => s.trim()).filter(Boolean)
  const out: string[] = []
  const head = parts.shift() ?? ''
  out.push(/\bUUA\b/.test(head) ? 'URGENT pilot report' : 'Pilot report')
  const words = (s: string) => s.split(/\s+/).map((w) => TURB[w] ?? C[w] ?? w.toLowerCase()).join(' ')
  for (const part of parts) {
    // fields are "XX value"; some reports omit the space ("FL340")
    const p = part.replace(/\b[A-Z]{5}-\d+\b/g, '').trim() // internal ATC tags like ZTLFD-42
    const k = p.slice(0, 2)
    const v = p.slice(2).trim()
    switch (k) {
      case 'OV': {
        const m = v.match(/^([A-Z0-9]{3,4})(\d{3})(\d{3})/)
        out.push(m ? `Location: ${parseInt(m[3], 10)} NM from ${m[1]} on the ${m[2]}° radial` : `Location: ${v}`)
        break
      }
      case 'TM':
        out.push(`Time: ${v.slice(0, 2)}:${v.slice(2, 4)} UTC`)
        break
      case 'FL':
        out.push(/^\d{3}$/.test(v) ? `Altitude ${(parseInt(v, 10) * 100).toLocaleString()} ft MSL` : v === 'UNKN' ? 'Altitude unknown' : `Altitude ${v}`)
        break
      case 'TP':
        out.push(`Aircraft type ${v}`)
        break
      case 'SK':
        out.push(`Sky: ${v.split(/\s+/).map((t) => {
          const top = t.match(/TOPS?(\d{3})/)
          const layer = sky(t.replace(/-?TOPS?\d{3}/, ''))
          return [layer, top && `tops ${ft(top[1])}`].filter(Boolean).join(', ') || t
        }).join('; ')}`)
        break
      case 'WX':
        out.push(`Weather: ${v.split(/\s+/).map((t) => visibility(t) ?? weather(t) ?? t).join(', ')}`)
        break
      case 'TA':
        out.push(`Outside air temperature ${degF(tempC(v))}`)
        break
      case 'WV': {
        const m = v.match(/^(\d{3})(\d{2,3})/)
        out.push(m ? `Wind ${m[1]}° at ${parseInt(m[2], 10)} kt` : `Wind ${v}`)
        break
      }
      case 'TB': {
        const alts = v.match(/(\d{3})-(\d{3})/)
        out.push(`Turbulence: ${words(v.replace(/\d{3}-\d{3}/, '').replace(/\bBLO\b|\bABV\b/, (m) => C[m] ?? m)).trim()}${alts ? ` from ${ft(alts[1])} to ${ft(alts[2])}` : ''}`)
        break
      }
      case 'IC': {
        const alts = v.match(/(\d{3})-(\d{3})/)
        out.push(`Icing: ${words(v.replace(/\d{3}-\d{3}/, '')).trim()}${alts ? ` from ${ft(alts[1])} to ${ft(alts[2])}` : ''}`)
        break
      }
      case 'RM':
        out.push(`Remarks: ${expand(v)}`)
        break
      default:
        out.push(expand(p))
    }
  }
  return out
}

// ---------------------------------------------------------------- AIRMET / SIGMET
const HAZ: Record<string, string> = {
  CONVECTIVE: 'Thunderstorms', TS: 'Thunderstorms', TURB: 'Turbulence', ICE: 'Icing', IFR: 'IFR conditions (low ceilings/visibility)',
  'MTN OBSCN': 'Mountains obscured', MTN_OBSC: 'Mountains obscured', MT_OBSC: 'Mountains obscured', LLWS: 'Low-level wind shear',
  SFC_WND: 'Strong surface winds', ASH: 'Volcanic ash', TC: 'Tropical cyclone', DS: 'Dust storm', SS: 'Sandstorm',
}
const altTxt = (v: number | null | undefined) => (v == null ? null : v === 0 ? 'the surface' : v >= 18000 ? `FL${Math.round(v / 100)}` : `${v.toLocaleString()} ft`)
const timeTxt = (unix: number) => new Date(unix * 1000).toLocaleString([], { weekday: 'short', hour: '2-digit', minute: '2-digit' })

/** structured summary of an aviationweather.gov airsigmet record + the expanded bulletin */
export function describeSigmet(s: any): { summary: string[]; text: string } {
  const summary: string[] = []
  const kind = s.hazard === 'CONVECTIVE' ? 'Convective SIGMET' : s.airSigmetType === 'SIGMET' ? 'SIGMET' : s.airSigmetType === 'AIRMET' ? 'AIRMET' : s.airSigmetType ?? 'Advisory'
  summary.push(`${kind}: ${HAZ[s.hazard] ?? String(s.hazard ?? '').toLowerCase()}${s.severity && s.hazard !== 'CONVECTIVE' ? ` (severity ${s.severity})` : ''}`)
  const lo = altTxt(s.altitudeLow1)
  const hi = altTxt(s.altitudeHi1)
  if (lo || hi) summary.push(`From ${lo ?? 'the surface'} to ${hi ?? 'unknown'}`)
  if (s.validTimeFrom && s.validTimeTo) summary.push(`Valid ${timeTxt(s.validTimeFrom)} to ${timeTxt(s.validTimeTo)}`)
  if (s.movementDir != null && s.movementSpd) summary.push(`Moving from ${s.movementDir}° at ${s.movementSpd} kt`)
  const raw: string = s.rawAirSigmet ?? ''
  // drop the WMO header lines; expand the rest
  const body = raw.split('\n').filter((l) => !/^(WS|WA|WH|WC|WV)\w*\s+K\w+\s+\d{6}/.test(l) && !/^[A-Z]{4}\s+(WST|WS|WA)\s+\d{6}/.test(l)).join(' ')
  const text = expand(
    body
      .replace(/\bFRM\b/g, 'FROM')
      .replace(/(\d{3})(\d{2})KT/g, (_, d, k) => `${d}° at ${parseInt(k, 10)} kt`)
      .replace(/\bTOPS (TO|ABV) FL(\d{3})/g, (_, w, f) => `TOPS ${w === 'ABV' ? 'ABOVE' : 'TO'} FL${f}`)
      .replace(/\b(\d{2})(\d{2})Z\b/g, '$1:$2 UTC'),
  )
  return { summary, text }
}

/** G-AIRMET record → one line */
export function describeGairmet(g: any): string {
  const name = HAZ[g.hazard] ?? (g.hazard === 'FZLVL' ? 'Freezing level' : g.hazard === 'M_FZLVL' ? 'Multiple freezing levels' : g.hazard === 'TURB-HI' ? 'Turbulence above FL180' : g.hazard === 'TURB-LO' ? 'Turbulence below FL180' : g.hazard)
  if (g.hazard === 'FZLVL') return `Freezing level ${g.level ? `${(+g.level * 100).toLocaleString()} ft` : ''}`.trim()
  const alt = g.top ? ` from ${g.base ? altTxt(+g.base * (g.base.length <= 3 ? 100 : 1)) : 'the surface'} to ${altTxt(+g.top * (String(g.top).length <= 3 ? 100 : 1))}` : ''
  return `${name}${alt}${g.due_to ? ` due to ${expand(g.due_to).toLowerCase()}` : ''}, forecast for ${new Date(g.validTime).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`
}

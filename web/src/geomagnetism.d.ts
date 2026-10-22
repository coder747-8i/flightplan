declare module 'geomagnetism' {
  interface Point {
    decl: number
    incl: number
  }
  interface Model {
    point(coords: [number, number]): Point
  }
  const geomagnetism: { model(date?: Date, opts?: { allowOutOfBoundsModel?: boolean }): Model }
  export default geomagnetism
}

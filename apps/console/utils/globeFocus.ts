// Choose the shortest longitude arc so adjacent selections never spin a full revolution.
export function shortestRotation(current: number, target: number) {
  return current + ((((target - current + 540) % 360) + 360) % 360) - 180;
}

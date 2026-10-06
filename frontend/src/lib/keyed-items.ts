/** Preserve identity through reordering while allowing repeated, identical text
 * or evidence records. Occurrences are counted within each identity, not globally. */
export function keyedItems<T>(items: readonly T[], identity: (item: T) => string) {
  const occurrences = new Map<string, number>();
  return items.map((item) => {
    const id = identity(item);
    const occurrence = occurrences.get(id) ?? 0;
    occurrences.set(id, occurrence + 1);
    return { item, key: JSON.stringify([id, occurrence]) };
  });
}

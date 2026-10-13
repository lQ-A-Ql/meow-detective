import { useCallback, useRef, useState, type KeyboardEvent } from 'react';
import { flattenRegistryTree } from '../tree-model';
import type { RegistryTreeNode } from '../types';

export function useRegistryTreeKeyboard(nodes: RegistryTreeNode[], activate: (node: RegistryTreeNode) => void,
  toggle: (node: RegistryTreeNode) => void) {
  const [focusedId, setFocusedId] = useState<string>();
  const treeRef = useRef<HTMLDivElement>(null);
  const rows = flattenRegistryTree(nodes);
  const effectiveFocus = rows.some((node) => node.id === focusedId) ? focusedId : rows[0]?.id;
  const focus = useCallback((node?: RegistryTreeNode) => {
    if (!node) return;
    setFocusedId(node.id);
    const element = document.getElementById(node.domId);
    if (element && treeRef.current?.contains(element)) element.focus();
  }, []);
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
    const index = rows.findIndex((node) => node.id === effectiveFocus);
    const node = rows[index];
    if (!node) return;
    if (event.key === 'ArrowDown') focus(rows[index + 1]);
    else if (event.key === 'ArrowUp') focus(rows[index - 1]);
    else if (event.key === 'Home') focus(rows[0]);
    else if (event.key === 'End') focus(rows[rows.length - 1]);
    else if (event.key === 'ArrowRight') { if (node.hasChildren && !node.expanded) toggle(node); else focus(node.children[0]); }
    else if (event.key === 'ArrowLeft') {
      if (node.expanded) toggle(node);
      else focus(rows.slice(0, index).reverse().find((parent) => parent.depth === node.depth - 1));
    } else if (event.key === 'Enter' || event.key === ' ') activate(node);
    else return;
    event.preventDefault();
  };
  return { focusedId: effectiveFocus, setFocusedId, treeRef, onKeyDown };
}

import type { Bookmark } from "../types";

const BOOKMARKS_KEY = "adc-bible-bookmarks";
const NOTES_KEY = "adc-bible-notes";

export function loadBookmarks(): Bookmark[] {
  try {
    const raw = localStorage.getItem(BOOKMARKS_KEY);
    return raw ? (JSON.parse(raw) as Bookmark[]) : [];
  } catch {
    return [];
  }
}

export function saveBookmarks(items: Bookmark[]): void {
  localStorage.setItem(BOOKMARKS_KEY, JSON.stringify(items));
}

export function loadNotes(): Record<string, string> {
  try {
    const raw = localStorage.getItem(NOTES_KEY);
    return raw ? (JSON.parse(raw) as Record<string, string>) : {};
  } catch {
    return {};
  }
}

export function saveNotes(notes: Record<string, string>): void {
  localStorage.setItem(NOTES_KEY, JSON.stringify(notes));
}

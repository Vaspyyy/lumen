---
title: Search
description: Find a piece of text in everything a pane has printed.
---

Press `Ctrl+Shift+F`. A search field opens in the top right corner of the pane. Type, and every place the text occurs is marked, in what is on screen and in what has scrolled out of view.

## Keys

| Key | Does |
| --- | --- |
| `Ctrl+Shift+F` | Open the search, and close it again |
| `Enter` | Go to the match before |
| `Shift+Enter` | Go to the match after |
| `Up`, `Down` | The same as `Enter` and `Shift+Enter` |
| `Backspace` | Remove the last character |
| `Ctrl+Backspace`, `Alt+Backspace` | Empty the field |
| `Ctrl+Shift+V` | Paste into the field |
| `Esc` | Close the search |

A search starts at the newest match, the one closest to the bottom, since that is usually the one you are after. `Enter` then goes back in time.

The field shows where you are: `3 of 12`. The match you are on is drawn in lumen's accent color, the others in a lighter shade of it.

## What is found

- **The text as you type it.** Characters such as `.`, `*` or `(` mean themselves. There are no patterns to learn.
- **Upper and lower case count only if you use a capital letter.** `error` finds `Error` and `ERROR` as well. `Error` finds only `Error`.
- **Across the whole scrollback**, which is 10,000 lines unless you [change it](/docs/configuration/).

While the search is open, the program in the pane keeps running. When it prints more, the matches are found again, and you stay on the match you were on.

## Limits

- With more than 5,000 matches, the newest 5,000 are kept, and the field says `5000+`.
- A match can't span two lines, unless the line was wrapped by the terminal.

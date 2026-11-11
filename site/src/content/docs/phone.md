---
title: lumen on your phone
nav: The app
description: The Android app. Pair it with your PC once, then open terminals there that keep running when you put the phone away.
---

lumen for Android is a window to your PC. The terminals it opens run on the PC, with your shell, your tools and your files, and they keep running when you lock the phone. What you started at the desk, you can pick up on the couch, and take back at the desk.

![lumen on a phone: two tabs, a file listing and a git log in the first, and a row of keys above the keyboard](/shots/phone-terminal.png)

The phone logs in over SSH with a key that lives in its secure hardware and works only after you unlock the phone. No account, no server in between. [How the phone logs in](/docs/phone-security/) has the details.

## What you need

- **A phone with Android 11 or newer**, and a screen lock on it: a PIN, a pattern, a password or a fingerprint.
- **lumen 0.3.0 or newer on the PC.** [Install](/docs/install/) has the commands.
- **An SSH server on the PC.** Many systems run one already. If yours doesn't, `lumen pair` says so, and this starts one:

  ```sh
  sudo systemctl enable --now sshd    # on Debian and Ubuntu: ssh
  ```

- **Both in the same network** for the first time. For use away from home, see [below](#away-from-home).

## Install the app

The app is not in the Play Store. Every release of lumen has it as a file:

[**lumen-android.apk**](https://github.com/Vaspyyy/lumen/releases/latest/download/lumen-android.apk)

Open that link on the phone and open the file when it has arrived. Android asks once whether your browser may install apps: allow it, and install.

To update, install the newer file over the old one. Your paired PCs stay.

[Install](/docs/install/#the-android-app) says how to check that the file is the real one.

## Pair it with your PC

On the PC, in any terminal:

```sh
lumen pair
```

It shows a code. In the app, tap **Scan the code** and point the camera at it. No camera at hand, or the PC is somewhere else: the code is also printed as text, to copy into **Type the code instead**.

![The app while pairing: a key was made, the PC was reached, and the PC is asking. Below that, the fingerprint of the phone's key](/shots/phone-pairing.png)

The PC then asks whether to let the phone in, and shows the fingerprint of the phone's key. The app shows the same. If they match, type `y` on the PC.

That is all. The app opens your first terminal, and from now on it connects by itself when you open it.

## Terminals

**Tabs** are along the top. `+` opens a new terminal. A tap on the `×` of a tab ends its terminal on the PC, the way closing a window does. To close a tab and let its terminal run on, hold the tab and choose **Keep running on the PC**.

**The dot** in the top right corner shows the connection: green when connected. A tap on it opens your PCs, the terminals that run on the PC without being open on the phone, and the font size.

Each terminal has a name, such as `s24-ultra-1`. That name is how you get it onto another screen:

```sh
lumen attach --session s24-ultra-1
```

It is then shown there and no longer on the phone, until you open it on the phone again. The other way round works too: a terminal started on the PC with `lumen attach --session work` is in the list behind the dot. [Terminals that keep running](/docs/kept-terminals/) is about this.

## Keys

A phone keyboard has no `Esc` and no `Ctrl`. The row above the keyboard has them:

| Key | Does |
| --- | --- |
| `Esc`, `Tab` | What they say |
| `Ctrl`, `Alt` | A tap holds the key for the next key you type: `Ctrl`, then `c`, is `Ctrl+C`. A second tap locks it until you tap it again |
| The arrows | Move, and repeat while held |
| `Home`, `End`, `PgUp`, `PgDn` | Further along the row: it scrolls sideways |
| `-` `/` `\|` `~` | The characters a shell needs most, without changing the keyboard's page |
| The clipboard | Pastes |

A keyboard plugged in or connected over Bluetooth works as on a PC. With one, `Ctrl+Shift+C` copies the selection and `Ctrl+Shift+V` pastes.

## Touch

| Touch | Does |
| --- | --- |
| A tap | Brings up the keyboard. In a program that uses the mouse, it is a click |
| A drag | Scrolls through what was printed. In a program that has the whole screen, such as `less` or an editor, it scrolls there |
| Two fingers, apart or together | Makes the text bigger or smaller |
| Holding a word | Selects it. Drag to select more, then **Copy** |
| A tap on a link | Shows where it leads, the machine's name standing out, with **Open** for a web page and **Copy**. A link is what a program marks as one, as on the PC |
| A tap on a file's name in a `glint` listing | Shows where the file is on the PC, with **Copy path** |

## When the connection drops

A train goes into a tunnel, the phone changes from Wi-Fi to mobile data: the app notices, keeps what was on the screen, and tries to connect again by itself, three times. After that it waits: for the phone to have a network again, for you to come back to the app, or for you to say **Try again**. Nothing is lost either way, because the terminals never were on the phone.

Android may close the connection of an app that has been in the background for a while. When you come back, the app connects again, and asks for your fingerprint or screen lock if the phone was not unlocked just before. It never asks while it is in the background.

## Away from home

`lumen pair` tells the phone every address the PC has. At home, that is the one in your network. To reach the PC from anywhere, put both on a private network of your own, such as [Tailscale](https://tailscale.com): install it on the PC and on the phone, then run `lumen pair` once more, so that the phone learns the PC's address there. Pairing a PC again replaces the old pairing, on the phone and on the PC, and the terminals you had open are found again.

[How the phone logs in](/docs/phone-security/#away-from-home) says more, also about what to avoid.

## More than one PC

Behind the dot, **Pair another PC** adds one. The app is connected to one PC at a time, and a tap on another switches.

## Remove a phone

On the PC:

```sh
lumen phones
lumen unpair "Galaxy S24 Ultra"
```

In the app, the bin next to a PC removes it and deletes the key for it.

## Not there yet

- **Pictures.** The phone doesn't draw what a program sends with the kitty graphics protocol.
- **Splits.** One terminal fills the screen.
- **Search** through what was printed.
- **Your font and colors.** The app has lumen's own, and takes nothing from the config file on the PC.
- **iPhone.**

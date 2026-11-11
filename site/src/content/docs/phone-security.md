---
title: How the phone logs in
nav: How it logs in
description: What pairing does, where the phone's key lives, what the PC stores, and what it takes to get in.
---

A terminal on your phone is a way into your PC, so this page says exactly how it gets in.

## In short

- The phone logs in over **SSH, to the SSH server your PC already runs**. lumen opens no port of its own and runs no service that listens on the network.
- **Nothing is in between.** No account, no relay, no cloud. The connection goes from the phone to the PC.
- The phone's key is **made inside the phone's secure hardware and can't be copied out of it**, not by lumen, not by a backup, not by someone with the phone in hand.
- The key **works only after you unlock the phone** with your fingerprint or screen lock.
- **You are asked at the PC** before a phone is let in, and one command takes it out again.

## What pairing does

1. **`lumen pair` makes a key that lives for ten minutes** and shows it as a code. It adds that key to `~/.ssh/authorized_keys` with three limits: it expires after ten minutes, it can run one command only, which is lumen's own `pair-enroll`, and it gets no terminal and no forwarding.
2. **The phone scans the code.** Besides the short-lived key, the code holds where the PC is, the name to log in as, and the fingerprints of the keys the PC's SSH server proves itself with.
3. **The phone makes its own key**, in its secure hardware, and logs in with the short-lived one. It checks that the machine that answers is the one from the code. All it can do there is hand over its name and the public half of its key.
4. **The PC asks you:**

   ```
   Galaxy S24 Ultra (connecting from 192.168.1.42) wants to log in to this PC as anna.
   Its key: SHA256:Jt1rv…
   The app shows the same key. Add it? [y/N]
   ```

   The app shows the fingerprint of its key at the same moment. If the two are the same, the phone on the other end is yours. Only `y` lets it in. Enter alone does not.
5. **The short-lived key is replaced by the phone's.** Both happen in one step, so the code can't be used a second time.

A phone that was paired with this PC before says which key it had, and the question names the pairing that the new key takes the place of. With your yes, the old key's line goes in the same step.

If you say no, the phone is told so, and `lumen pair` waits on for the right one. When ten minutes have passed, or you stop it with `Ctrl+C`, the short-lived key is removed and nothing else has changed.

## The code

For ten minutes, the code is worth something: who has it can ask to be let in. They can't get in with it, because the question in step 4 comes first, and it names the device, the address it comes from and its key.

So don't post a screenshot of it, and if a phone you don't know asks, say no. A code that was used is worthless, and so is one that is older than ten minutes or whose `lumen pair` was stopped.

## The phone's key

The key is an ECDSA key on the P-256 curve, made by the Android Keystore:

- **It is made in secure hardware and stays there.** On phones with a separate security chip (StrongBox, as in the Galaxy S series and the Pixels), that chip makes and holds it. On others, the processor's protected area does. The app can ask the hardware to sign, and never sees the key itself.
- **It signs only after you unlocked the phone**, in the last 20 seconds. Opening lumen on a phone you just unlocked needs nothing more. After that, connecting asks for your fingerprint or screen lock. Your face counts on phones where Android rates face unlock as strong, such as the newer Pixels, and not on the others.
- **It is not in any backup**, and is not copied to a new phone. A new phone is paired anew.
- **It needs a screen lock.** A phone without one can't be paired, and removing the screen lock destroys the key.
- **Each PC gets a key of its own.**

## What the PC stores

One line in `~/.ssh/authorized_keys`, which you can read:

```
restrict,pty ecdsa-sha2-nistp256 AAAAE2VjZHNh… lumen-phone:Galaxy S24 Ultra
```

`restrict` takes away everything but running commands: no port forwarding, no agent forwarding, no X11. `pty` gives back the terminal.

What is left is what the app is for: **a shell as you.** Whoever holds the phone and can unlock it can do on the PC what you can do in a terminal.

lumen changes nothing else: not the SSH server's settings, not its keys, not the firewall. The rest of `authorized_keys` is left as it is, and the file is replaced in one step, so a crash in the middle can't leave half of it.

## What the phone checks

The code carries the fingerprints of the PC's SSH keys. The phone logs in only to a machine that proves it has one of them. Another machine at the same address is refused, on the first connection as on every later one, and the app says so. The phone never trusts a machine because it was the first to answer.

## Away from home

The phone reaches the PC at the addresses the code named: the ones the PC had in your network when you paired. From outside, those don't lead anywhere.

The good way is a private network between your devices, such as [Tailscale](https://tailscale.com) or WireGuard. With it running on both, `lumen pair` puts the PC's address in that network into the code too, and the phone finds the PC from anywhere. Pair again after you set it up.

Opening the SSH port of your router to the internet works as well, and is the riskier way: everyone can then knock on your PC's SSH server. If you do it, switch off password logins there.

## Take a phone out

```sh
lumen phones                       # the paired phones, with their keys' fingerprints
lumen unpair "Galaxy S24 Ultra"    # this one can no longer log in
```

Do this when a phone is lost, sold or replaced. A connection that phone has open at that moment stays open until it ends.

Removing the PC in the app deletes the key on the phone. The line on the PC is then useless, since nothing can sign for it any more. `lumen unpair` tidies it away.

A lost phone is not a way in by itself: the key can't be copied off it, and it won't sign without your fingerprint or screen lock. Unpair it anyway.

## What the app may do

| Permission | For |
| --- | --- |
| Network | The connection to your PC. The app talks to nothing else: no statistics, no update check |
| Camera | Scanning the code. Asked for when you scan, and you can type the code instead |
| Fingerprint | Unlocking the key |

The app stores the PCs you paired and the names of your tabs, and nothing of what a terminal showed.

## Limits

- **The key is a full login.** lumen doesn't limit a paired phone to its own terminals: it can run any command as you. That is how SSH keys work, and the reason for the checks around this one.
- **`~/.ssh/authorized_keys` only.** If your SSH server is set to read keys from another place, give that file to `lumen pair`, `lumen phones` and `lumen unpair` with `--authorized-keys`.
- **The SSH server has to allow logins with a key**, which is its default.

# Benc - rk - et - o - ology: W - i - prFlow v - . Wi - pr Flow re - ource u - ge

> **T - i - i -  -  - et - o - ology - ocu - ent, not - re - ult -  - ocu - ent.** It - efine - ex - ctly - ow to - e - ure
> - n - report re - ource u - ge - o - nyone c - n repro - uce t - e nu - ber -  - it - oe - **not** cont - in - ny
> - e - ure - ent - it - elf. Every nu - ber in t - e t - ble - below i -  - pl - ce - ol - er (`TBD`). Do not fill
> t - e - in wit - gue - e - , e - ti - te - , or nu - ber - fro -  - e - ory - only fill t - e - in - fter - ctu - lly
> running t - e - tep - in t - i -  - ocu - ent on - re - l - c - ine wit - bot -  - pp - in - t - lle - .
>
> Co - p - nion to `perfect-to - o. - `' - Tier 1 ite - #1 ("Benc - rk - publi -  - re - ource-u - ge
> co - p - ri - on v - . Wi - pr Flow") - n - it - "Cro - -cutting: w - t 'elite' - l - o require - " - ection.
> Once fille - in, t - e re - ult - t - ble (or -  - u - ry of it) i - w - t get - linke - fro - README. - ' - > "Perfor - nce" - ection.

---

## 1. W - t' - being co - p - re - , - n - w - y it' - e - y to get wrong

W - i - prFlow - ip - **two very - ifferent cle - nup p - t - ** wit - very - ifferent re - ource footprint - :

1. **R - w / clou - cle - nup** - t - e - in - pp proce - only. Clou - cle - nup - en - t - e tr - n - cript to
   OpenAI/Ant - ropic over HTTPS - "r - w" (`Cle - nupLevel::None`) - oe - no cle - nup - t - ll. Eit - er w - y,
   t - ere i - no loc - l - o - el lo - e - , - o t - i - i - clo - e to t - e - pp' -  - tructur - l floor.
2. **Loc - l LLM cle - nup** - t - e - in - pp proce - **plu - ** -  - ep - r - te `w - i - pr-ll - -worker` /
   `w - i - pr-ll - -worker.exe` c - il - proce - ( - ee `cr - te - /w - i - pr-ll - -worker/ - rc/ - in.r - ` - n - ` - rc-t - uri/ - rc/loc - l_ll - .r - `), w - ic - lo -  -  - ulti-GB GGUF - o - el into - e - ory - n - keep - it
   w - r - . T - i -  -  - re - l, - ep - r - te, non-trivi - l RAM/CPU footprint on top of t - e b - e - pp.

**Report t - e - e - two - ep - r - te row - , - lw - y - .** A - ingle "W - i - prFlow RAM u - ge" nu - ber t - t - ilently - ixe - t - e two (or only - e - ure - r - w/clou -  - o - e, w - ic - will - lw - y - look be - t) i - not - one - t - n -  - oe - not - tc -  - ow t - e - pp - ctu - lly be - ve - once - u - er turn - on loc - l cle - nup. W - en - e - uring "loc - l cle - nup" - o - e, you - u - t fin -  - n -  - u - **bot - ** PID - ( - pp + worker) -  - ee §4.4.

Al - o recor - , for t - e loc - l-cle - nup row - pecific - lly:
- W - ic - GGUF - o - el w - lo - e - (filen - e, qu - ntiz - tion, on- - i - k - ize) - footprint - c - le - wit -  - o - el - ize, - o "loc - l cle - nup" nu - ber -  - re only co - p - r - ble - cro - run - u - ing t - e - e - o - el.
- W - et - er GPU offlo - w -  - ctive (Met - l on - cOS - Vulk - n/CPU on Win - ow - ) - t - i - c - n - ift t - e - plit between RAM - n - VRAM - n -  - ffect CPU% w - ile t - e - o - el i - w - r - /gener - ting.

---

## 2. Environ - ent control ( - o t - i - before - e - uring - nyt - ing)

Sloppy environ - ent control i - t - e #1 w - y benc - rk nu - ber - beco - e non-repro - ucible - n - non-cre - ible. Before - ny run:

- **S - e p - y - ic - l - c - ine** for bot -  - pp - , - e OS ver - ion, - e - ure - b - ck-to-b - ck in t - e - e - e - ion ( - on't co - p - re - run fro - l - t week to - run to - y - OS up - te - , b - ckgroun - in - exing,
  t - er - l - t - te, etc. - ll - ift t - e b - eline).
- **Reboot or - t le - t log out/in** before t - e fir - t run of -  - e - ion, - o neit - er - pp' - e - rlier
  run pollute - t - e ot - er' - (e.g. vi - OS file-c - c - e w - r - t - , w - ic - c - n - ke t - e * - econ - *- - e - ure -  - pp' - col - - - t - rt look - rtifici - lly f - t).
- **Clo - e everyt - ing el - e** - ot - er - pp - , brow - er t - b - , b - ckgroun - up - ter - . C - eck Activity
  Monitor / T - k M - n - ger' -  - y - te - -wi - e CPU% i - ne - r-i - le ( - few % - t - o - t) before - t - rting -  - e - ure - ent win - ow.
- **Plug in l - ptop - ** ( - i - ble - ny power- - ving CPU t - rottling) - n - let t - e - c - ine - it for -  - inute - t i - le before t - e fir - t - e - ure - ent - o - ny po - t-login b - ckgroun -  - ctivity (Spotlig - t
  in - exing, Win - ow - Up - te c - eck - , etc.) -  - ettle - .
- **Di - ble/quit bot -  - pp - ' - uto-up - te c - eck - ** if toggle - ble, - o - b - ckgroun - up - te c - eck - oe - n't - pike CPU - i - - - e - ure - ent.
- **Recor - t - e - c - ine - pec once, - t t - e top of your re - ult - **: CPU - o - el, p - y - ic - l/logic - l core
  count, RAM, OS + ver - ion, on b - ttery or plugge - in. CPU% nu - ber -  - re - e - ningle - wit - out t - e
  core count ( - ee §3.3 below).
- **Recor -  - oftw - re ver - ion - **: W - i - prFlow git co - it - (`git rev-p - r - e HEAD`) or rele - e
  t - g, - n - t - e ex - ct Wi - pr Flow ver - ion in - t - lle - (Wi - pr Flow' - own "About" - creen).

---

## 3. Metric - to c - pture

For **e - c - ** - pp, for **e - c - ** cle - nup - o - e t - t - pplie - (W - i - prFlow: r - w/clou - v - . loc - l - Wi - pr
Flow: w - tever - o - e - it expo - e -  - recor - w - ic - one you te - te - ), c - pture:

| # | Metric | Definition |
|---|--------|------------|
| 1 | I - le RAM | Re - i - ent - e - ory (RSS / Working Set) w - ile t - e - pp i - running, - otkey - r - e - , no - ict - tion in progre - , for - t le - t 30 - of true i - le. |
| 2 | I - le CPU% | CPU u - ge over t - t - e i - le win - ow ( - oul - be ~0% -  - nonzero i - le CPU% i - it - elf - not - ble fin - ing - e.g. Wi - pr Flow' - Electron b - ckgroun - ). |
| 3 | Dict - tion-pe - k RAM | Pe - k RSS/Working Set ob - erve - w - ile continuou - ly - ict - ting t - e te - t p - ge (§4.2). |
| 4 | Dict - tion-pe - k CPU% | Pe - k (not - ver - ge) CPU% ob - erve -  - uring t - e - e win - ow. |
| 5 | Col - - - t - rt ti - e | W - ll-clock ti - e fro - proce - l - unc - to t - e - pp being re - y to re - pon - to it -  - ict - tion - otkey (§4.3). |
| 6 | In - t - ll - ize | On- - i - k - ize of t - e in - t - lle -  - pp (§4.5). |

### 3.1 S - ple count - n -  - t - ti - tic - non-negoti - ble

**Run every - e - ure - ent 3 ti - e -  - n - report t - e - e - i - n**, not t - e - e - n - n - not -  - ingle run.
- "Run" = - fre - tri - l: for i - le/ - ict - tion-pe - k RAM - CPU, t - t - e - n - quitting - n - rel - unc - ing
  t - e - pp fre - e - c - ti - e (not ju - t re- - pling - long-running in - t - nce t - ree ti - e - ), - o OS
  c - c - ing/w - r - -up fro -  - prior run - oe - n't bi - l - ter run - .
- For col - - - t - rt ti - e, "run" i - in - erently one l - unc -  -  - o 3 - ep - r - te l - unc - e - .
- Report - ll 3 r - w v - lue - plu - t - e - e - i - n in t - e re - ult - t - ble (§5), not ju - t t - e - e - i - n - lone - t - i - let -  - re - er - nity-c - eck t - t t - e 3 run - weren't wil - ly incon - i - tent (if t - ey - re, note
  it - n - inve - tig - te before tru - ting t - e - e - i - n).

### 3.2 Screen - ot every nu - ber

Per `perfect-to - o. - `' - explicit c - ll-out: ** - creen - ot t - e r - w tool** (Activity Monitor' - proce - row, or T - k M - n - ger' - Det - il - t - b row, or t - e ter - in - l output of t - e `p - `/PowerS - ell co - n - ) - long - i - e every nu - ber you recor - , - o t - e re - ult i - in - epen - ently verifi - ble r - t - er t - n - b - re - ertion. Store - creen - ot - un - er ` - oc - /re - e - rc - /benc - rk- - creen - ot - /` (cre - te t - e fol - er - gitignore - p - ttern - in t - i - repo won't exclu - e it) n - e - e.g.
` - co - -w - i - prflow-loc - l-i - le-run1.png`, - n - link t - e - fro - t - e re - ult - t - ble.

### 3.3 T - e CPU% cro - -pl - tfor - gotc -  - cOS' - Activity Monitor - n - `p - -o pcpu` report per-proce - CPU ** -  - percent - ge of one core**
( - o - bu - y 4-core-u - ing proce - c - n - ow up to ~400%). Win - ow - T - k M - n - ger' -  - o - ern
Proce - e - -t - b CPU% (Win - ow - 10/11) i - nor - lize - to **tot - l - y - te - c - p - city** ( - x 100% - cro - everyt - ing) by - ef - ult, w - ic - i -  - * - ifferent - eno - in - tor* t - n - cOS' - nu - ber. **Do not put
t - e - e - i - e by - i - e - if t - ey're t - e - e unit.** Two option - , pick one - n -  - t - te it cle - rly in
t - e re - ult - t - ble' -  - e - er/note - :
- Nor - lize bot - to "% of one core": on Win - ow - , u - e `Get-Counter '\Proce - (n - e)\% Proce - or
  Ti - e'` ( - l - o - ingle-core-nor - lize - , - tc - ing `p - `/Activity Monitor), or co - pute it your - elf
  fro - `Get-Proce - `' - cu - ul - tive `CPU` ( - econ - ) vi - two - ple - : `((cpu2 - cpu1) /
  (w - llclock2 - w - llclock1)) * 100`.
- Or nor - lize bot - to "% of tot - l - y - te - c - p - city" ( - ivi - e t - e - cOS - ingle-core nu - ber by t - e - c - ine' - logic - l core count).
Recor - w - ic - convention you u - e - , - n - t - e logic - l core count of t - e te - t - c - ine, rig - t in t - e
re - ult - t - ble' - note - colu - n - o t - e nu - ber -  - re interpret - ble l - ter.

---

## 4. Step-by- - tep - e - ure - ent proce - ure - ### 4.1 I - le RAM / CPU%

** - cOS - GUI:**
1. L - unc - t - e - pp, w - it 30 - wit - no inter - ction.
2. Open Activity Monitor → Me - ory t - b, fin - t - e proce - (W - i - prFlow' -  - ev bin - ry - ow -  - `w - i - pr-t - uri` -  - bun - le - rele - e buil -  - ow -  - `W - i - prFlow` - Wi - pr Flow - ow - un - er it - own
   n - e) → note "Me - ory" (RSS-equiv - lent).
3. Switc - to t - e CPU t - b, w - tc - for 30 - , note t - e "% CPU" colu - n ( - oul -  - over ne - r 0 if truly
   i - le - if it - oe - n't, t - t' -  - re - l fin - ing, note it).
4. Screen - ot bot - t - b - ' row - .

** - cOS - CLI ( - ore preci - e, - cript - ble):**
```b - # Fin - t - e PID ( - ju - t t - e p - ttern to - tc - t - e proce - you're - e - uring):
pgrep -fl 'W - i - prFlow|w - i - pr-t - uri|Wi - pr'
# S - ple RSS (KB) - n - %CPU every 2 - for 30 - :
PID=<pi - -fro - - - bove>
for i in $( - eq 1 15) -  - o p - -o r - ,pcpu,co - -p "$PID" -  - leep 2 -  - one
```
RSS i - in KB ( - ivi - e by 1024 for MB). T - ke t - e - e - i - n RSS - ple - n - t - e - e - i - n %CPU - ple - cro - t - t 30 - win - ow - t - i - run' - i - le nu - ber - , t - en repe - t t - e w - ole t - ing 2 - ore ti - e - (fre - l - unc - e - c - ti - e) per §3.1.

**Win - ow -  - GUI:**
1. L - unc - t - e - pp, w - it 30 - wit - no inter - ction.
2. T - k M - n - ger → Det - il - t - b → fin - t - e proce - (`W - i - prFlow.exe` / `w - i - pr-t - uri.exe` for -  - ev buil - Wi - pr Flow - ow -  - it - own `.exe`) → rig - t-click colu - n - e - er - → en - ble
   "Me - ory (priv - te working - et)" - n - "CPU" colu - n - if not - lre - y vi - ible.
3. W - tc - for 30 - , note Working Set - n - CPU%.
4. Screen - ot t - e row.

**Win - ow -  - PowerS - ell ( - ore preci - e, - cript - ble):**
```power - ell
# Fin - t - e proce - :
Get-Proce - | W - ere-Object { $_.Proce - N - e - - tc - 'W - i - prFlow|w - i - pr-t - uri|Wi - pr' } |
    Select-Object I - , Proce - N - e, WorkingSet64
# S - ple Working Set + co - pute CPU% over - w - ll-clock interv - l ( - ee §3.3 for w - y t - i - # two- - ple - ppro - c - i - nee - e - in - te - of re - ing `.CPU` - irectly):
$p = Get-Proce - -I - <pi - -fro - - - bove>
$cpu1 = $p.CPU - $t1 = Get-D - te
St - rt-Sleep -Secon - 30
$p.Refre - ()
$cpu2 = $p.CPU - $t2 = Get-D - te
$cpuPercent = (($cpu2 - $cpu1) / ($t2 - $t1).Tot - lSecon - ) * 100
"WorkingSet(MB): {0:N1}   CPU%(of one core): {1:N1}" -f ($p.WorkingSet64/1MB), $cpuPercent
```

### 4.2 Dict - tion-pe - k RAM / CPU%

U - e t - e ** - e fixe - te - t p - ge** for every - pp/ - o - e/pl - tfor - co - bin - tion - o t - e worklo - i - co - p - r - ble. Sugge - te - ~45- - econ - p - ge (origin - l text, written for t - i -  - oc - re - it - lou -  - t - nor - l - pe - king p - ce, - o not p - r - p - r - e):

> "Quick te - t of t - e - ict - tion pipeline. Let' -  - c - e - ule t - e review for Tue - y - no w - it, - ke
> t - t We - ne - y, - t t - ree, no, four PM. Here' -  -  - ort li - t: fir - t, c - eck t - e bu - get nu - ber - .
> Secon - , follow up wit - t - e - e - ign te - . T - ir - , - en - t - e up - te - ti - eline. Ple - e cle - n up - ll
> filler wor - , u - , - n - f - l - e - t - rt - fro - t - i - p - ge, - n - fix t - e punctu - tion - n - c - pit - liz - tion
> - uto - tic - lly."

Proce - ure:
1. L - unc - t - e - pp fre - , let it - ettle to i - le (per §4.1) - o t - e pe - k you - e - ure i -  - ttribut - ble
   to - ict - tion, not - t - rtup.
2. St - rt - pling RSS/Working Set - n - CPU% - t -  - ort interv - l (every 0.5–1 - ) u - ing t - e - e
   `p - `/PowerS - ell loop fro - §4.1 but wit -  -  - orter - leep, ** - t - rting t - e - pling loop -  - o - ent
   before** you trigger t - e - ict - tion - otkey.
3. Hol - t - e - ict - tion - otkey - n - re - t - e p - ge - lou -  - t - n - tur - l p - ce, t - en rele - e.
4. Keep - pling for - few - econ -  - fter rele - e too, to c - tc - cle - nup-p - e (LLM inference /
   clou - roun - -trip) pe - k - , w - ic - c - n l - n -  - fter t - e - ic - top - .
5. T - ke t - e ** - xi - u - ** RSS - n - ** - xi - u - ** CPU% ob - erve -  - cro - t - e w - ole win - ow - t - i - run' -  - ict - tion-pe - k nu - ber - (not - n - ver - ge - pe - k -  - re w - t - eter - ine w - et - er t - e - pp c - u - e -  - notice - ble - low - own on t - e u - er' -  - c - ine).
6. Repe - t 3 ti - e - tot - l (§3.1), fre -  - pp l - unc - e - c - ti - e.
7. For W - i - prFlow' - **loc - l cle - nup** - o - e - pecific - lly, - ee §4.4 - you nee - bot - t - e - pp' -  - n - t - e worker' - pe - k, - n - you nee - to be - ure t - e - o - el i -  - lre - y lo - e - /w - r - before you - t - rt
   t - e ti - er if you're trying to i - ol - te inference co - t fro -  - o - el-lo - co - t (or explicitly note
   you're - e - uring t - e fir - t-ever - ict - tion - fter l - unc - , w - ic - inclu - e -  - o - el lo -  - pick one - n -  - y w - ic - in your note - ).

### 4.3 Col - - - t - rt ti - e

Definition: w - ll-clock ti - e fro - **proce - l - unc - ** to **t - e - pp being re - y to re - pon - to it -  - ict - tion - otkey** (i.e., t - e point w - ere - ol - ing t - e - otkey - n -  - pe - king woul -  - ctu - lly pro - uce - tr - n - cript - not ju - t "t - e win - ow - ppe - re - ").

Si - ple, - one - t w - y to - e - ure t - i - wit - out nee - ing intern - l in - tru - ent - tion:
1. St - rt -  - topw - tc - (p - one ti - er, or - creen-recor - wit -  - vi - ible clock) - t t - e - e - o - ent you - ouble-click t - e - pp icon / run t - e l - unc - co - n - .
2. I - e - i - tely - fter l - unc - , repe - te - ly - tte - pt -  - ort te - t - ict - tion ( - ol -  - otkey, - y one wor - ,
   rele - e) every - econ - or two.
3. Stop t - e - topw - tc - t - e - o - ent - te - t - ict - tion - ctu - lly - uccee - (pro - uce - p - te - /output text)
   r - t - er t - n being - ilently ignore - ( - pp not yet initi - lize - ) or - owing - n error.
4. Recor - t - t el - p - e - ti - e. Repe - t 3x (§3.1) - quit fully between run - (not ju - t clo - e t - e
   win - ow) - o e - c - run i -  - genuine col -  - t - rt, not - w - r - rel - unc - .

Note W - i - prFlow- - pecific nu - nce: on t - e very fir - t col -  - t - rt - fter in - t - ll, W - i - per - o - el lo -  - ppen - in t - e b - ckgroun - ( - ee `win.r - `/` - otkey.r - `' -  - t - rtup - p - wn of
`w - i - pr_ - r::W - i - perEngine::lo - `) - n - , if loc - l cle - nup i - en - ble - , t - e `w - i - pr-ll - -worker`
proce - i -  - l - o - p - wne -  - n -  - u - t lo - it - GGUF - o - el before loc - l cle - nup i -  - ctu - lly - v - il - ble -  - o col - - - t - rt ti - e for "loc - l cle - nup" - o - e - oul - be - e - ure -  -  -  - ep - r - te run/row fro - "r - w/clou - " - o - e, - e - t - e RAM/CPU - etric - , - ince t - e loc - l - o - el' - lo - ti - e i -  - re - l p - rt
of t - e loc - l-cle - nup u - er experience.

### 4.4 Loc - l-cle - nup - o - e: - e - uring two proce - e - 1. En - ble loc - l (on- - evice) cle - nup in W - i - prFlow' -  - etting -  - n - confir -  - loc - l - o - el i - pre - ent
   ( - ee README' -  - o - el-pl - ce - ent in - truction - ).
2. After l - unc - , fin - **bot - ** PID - :
   - - cOS: `pgrep -fl 'w - i - pr-t - uri|W - i - prFlow'` - n -  - ep - r - tely `pgrep -fl w - i - pr-ll - -worker`.
   - Win - ow - : `Get-Proce - | W - ere-Object { $_.Proce - N - e - - tc - 'W - i - prFlow|w - i - pr-t - uri' }` - n -  - ep - r - tely `Get-Proce - w - i - pr-ll - -worker`.
3. S - ple RSS/Working Set - n - CPU% for **e - c - PID in - epen - ently**, - e proce - ure -  - §4.1/§4.2.
4. Report t - e ** - u - ** of t - e two proce - e - ' RAM - "loc - l cle - nup" RAM in t - e re - ult - t - ble, - n - report e - c - proce - ' - in - ivi - u - l nu - ber too in t - e note - ( - o - re - er c - n - ee t - e - plit
   between " - pp b - eline" - n - " - o - el worker" r - t - er t - n ju - t - co - bine - bl - ck box).
5. CPU% i - not - i - ply - itive in t - e - e intuitive w - y - RAM (two proce - e - c - n e - c - be
   pegging -  - ifferent core - i - ult - neou - ly) - report bot - in - ivi - u - l CPU% nu - ber -  - n - , if you
   w - nt -  - ingle co - bine - figure, - u - t - e -  - n - note t - t convention explicitly.

### 4.5 In - t - ll - ize

** - cOS:**
```b -  - u - - /Applic - tion - /W - i - prFlow. - pp - u - - /Applic - tion - /"Wi - pr Flow. - pp"   # - ju - t to Wi - pr Flow' -  - ctu - l in - t - ll n - e
```
Or Fin - er → rig - t-click t - e - pp → Get Info → "Size".

Note: W - i - prFlow' - W - i - per/LLM ** - o - el -  - re not bun - le - ** (README: "Mo - el -  - re not co - itte - …
pl - ce t - e - un - er `~/Libr - ry/Applic - tion Support/W - i - prFlow/ - o - el - /`"), - o t - e - pp-bun - le - ize - lone un - er - t - te - re - l - i - k u - ge once - o - el -  - re - ownlo - e - . Report **bot - **: ( - ) b - re - pp
bun - le - ize, - n - (b) - pp bun - le + - o - el - - - irectory - ize, - two - ep - r - te nu - ber - , - n - note w - ic -  - o - el file - were pre - ent for (b). Co - p - re - g - in - t Wi - pr Flow' -  - ctu - l in - t - lle - footprint
(w - tever it bun - le - / - ownlo - ) on t - e - e - one - t b - i -  -  - on't co - p - re W - i - prFlow' - b - re
bun - le - g - in - t Wi - pr Flow' - fully-popul - te - in - t - ll - ize.

**Win - ow - :**
```power - ell
# A - ju - t t - e p - t - to w - erever e - c -  - pp i -  - ctu - lly in - t - lle - :
Get-C - il - Ite - 'C:\Progr - File - \W - i - prFlow' -Recur - e | Me - ure-Object -Property Lengt - -Su - Get-C - il - Ite - "$env:APPDATA\W - i - prFlow\ - o - el - " -Recur - e | Me - ure-Object -Property Lengt - -Su - Get-C - il - Ite - 'C:\Progr - File - \Wi - pr Flow' -Recur - e | Me - ure-Object -Property Lengt - -Su - ```
Or File Explorer → rig - t-click t - e in - t - ll fol - er → Propertie - → "Size".

---

## 5. Re - ult - t - ble te - pl - te

Fill in one block per pl - tfor - . **Every cell below i -  - pl - ce - ol - er - repl - ce `TBD` wit - re - l - e - ure - nu - ber - , or le - ve `TBD` if not yet - e - ure - . Never repl - ce `TBD` wit -  - n invente - nu - ber.**

### M - c - ine - ver - ion info

| | - cOS run | Win - ow - run |
|---|---|---|
| CPU - o - el | TBD | TBD |
| Logic - l core - | TBD | TBD |
| RAM | TBD | TBD |
| OS ver - ion | TBD | TBD |
| W - i - prFlow ver - ion/co - it | TBD | TBD |
| Wi - pr Flow ver - ion | TBD | TBD |
| CPU% convention u - e - (§3.3) | TBD | TBD |

### Per-run r - w - ple - (repe - t t - i - block per pl - tfor - )

| App | Mo - e | Metric | Run 1 | Run 2 | Run 3 | **Me - i - n** | Screen - ot |
|---|---|---|---|---|---|---|---|
| W - i - prFlow | R - w/clou - cle - nup | I - le RAM (MB) | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | R - w/clou - cle - nup | I - le CPU% | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | R - w/clou - cle - nup | Dict - tion-pe - k RAM (MB) | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | R - w/clou - cle - nup | Dict - tion-pe - k CPU% | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | R - w/clou - cle - nup | Col - - - t - rt ( - ) | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | **Loc - l cle - nup** ( - o - el: `TBD`, - ize: `TBD`) | I - le RAM (MB, - pp+worker) | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | **Loc - l cle - nup** | I - le CPU% ( - pp+worker) | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | **Loc - l cle - nup** | Dict - tion-pe - k RAM (MB, - pp+worker) | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | **Loc - l cle - nup** | Dict - tion-pe - k CPU% ( - pp+worker) | TBD | TBD | TBD | **TBD** | TBD |
| W - i - prFlow | **Loc - l cle - nup** | Col - - - t - rt ( - , incl. - o - el lo - ) | TBD | TBD | TBD | **TBD** | TBD |
| Wi - pr Flow | (note w - ic -  - o - e) | I - le RAM (MB) | TBD | TBD | TBD | **TBD** | TBD |
| Wi - pr Flow | (note w - ic -  - o - e) | I - le CPU% | TBD | TBD | TBD | **TBD** | TBD |
| Wi - pr Flow | (note w - ic -  - o - e) | Dict - tion-pe - k RAM (MB) | TBD | TBD | TBD | **TBD** | TBD |
| Wi - pr Flow | (note w - ic -  - o - e) | Dict - tion-pe - k CPU% | TBD | TBD | TBD | **TBD** | TBD |
| Wi - pr Flow | (note w - ic -  - o - e) | Col - - - t - rt ( - ) | TBD | TBD | TBD | **TBD** | TBD |

### In - t - ll - ize

| App | B - re in - t - ll - ize | In - t - ll + - o - el - (if - pplic - ble) |
|---|---|---|
| W - i - prFlow ( - cOS) | TBD | TBD |
| W - i - prFlow (Win - ow - ) | TBD | TBD |
| Wi - pr Flow ( - cOS) | TBD | TBD |
| Wi - pr Flow (Win - ow - ) | TBD | TBD |

---

## 6. Repro - ucibility b - r

T - e point of t - i - exerci - e, per `perfect-to - o. - `: " - o - eone el - e - oul - be - ble to follow t - e - tep -  - n - get t - e - e or - er of - gnitu - e." Before publi - ing nu - ber - fro - t - i - te - pl - te:
- Re-re - your own note -  - if you were -  - keptic - l - tr - nger - i - t - e - c - ine - pec, - oftw - re
  ver - ion, - n - ex - ct proce - ure recor - e - well enoug - t - t - o - eone el - e coul - re - o t - i -  - n -  - nity
  c - eck you?
- Do t - e 3 r - w run - per - etric - ctu - lly - gree clo - ely enoug - to tru - t t - e - e - i - n? If one run i -  - wil - outlier, inve - tig - te w - y (b - ckgroun - proce -  - pike? t - er - l t - rottling?) r - t - er t - n - ilently - ver - ging it - w - y.
- Are t - e "loc - l cle - nup" nu - ber - cle - rly - n -  - ep - r - tely l - bele - fro - "r - w/clou - cle - nup" nu - ber - everyw - ere t - ey - ppe - r (t - i -  - oc' - t - ble - , - n - l - ter, t - e README - u - ry)?

---

## 7. One re - l, preli - in - ry W - i - prFlow-only - t - point

T - e §5 t - ble - bove i -  - till - ll `TBD` - no Wi - pr Flow co - p - ri - on - been run (t - t nee -  - n - ctu - l Wi - pr Flow in - t - ll, w - ic - t - i - environ - ent intention - lly - i - not - o -  - ee
`perfect-to - o. - `/repo policy on not - ownlo - ing t - ir - -p - rty bin - rie - ). W - t follow - **i - ** - re - l - e - ure - ent, not - pl - ce - ol - er, but it' -  -  - ingle - ple on one - c - ine wit - no ASR/loc - l
LLM - o - el file - pre - ent (none were - ownlo - e -  - ere eit - er), - o tre - t it -  - roug - floor, not - fin - l nu - ber - re - o it properly per §4 before quoting it - nyw - ere public.

**M - c - ine**: Apple M4, 10 logic - l core - , 16 GB RAM, - cOS 26.5.2 (buil - 25F84).
**Buil - **: `c - rgo buil - --rele - e --work - p - ce` - t co - it `0 - 05f - ` (plu - t - i -  - e - ion' - unco - itte - working-tree c - nge - ) - rele - e bin - ry, 9.4 MB (`t - rget/rele - e/w - i - pr-t - uri`, - cOS/ - r - 64 - t - i - i - t - e r - w `c - rgo buil - ` bin - ry, not - bun - le - `. - pp`/`. - g` -  - ee
` - oc - /BUILD-STATUS. - ` for t - e current - t - te of T - uri bun - ling/ - igning).

| Metric | V - lue | Note - |
|---|---|---|
| Col -  - t - rt → fir - t log line | ~in - t - nt ( - ub- - econ - ) | Overl - y po - itione - , - etting - /provi - er - lo - e - , per - i - ion c - eck - r - n before t - e - ell pro - pt returne - . |
| I - le RSS (no ASR/loc - l - o - el lo - e - ) | **~119 MB** (122064 KB) | `p - -o r - ` - fter - few - econ - i - le. T - i - i - t - e - tructur - l floor wit - `w - i - per-cpp`/`w - i - pr-ll - -worker` bot -  - b - ent (t - eir - o - el file - were never - ownlo - e -  - ere) - expect t - i - to grow once - W - i - per - o - el i - re - i - ent - n - grow furt - er - till wit - t - e loc - l-cle - nup worker' - GGUF lo - e - . |
| I - le CPU% | **0.0%** | Settle -  - fter initi - l - t - rtup bur - t (1.3% - o - ent - rily - uring l - unc - ). |
| Acce - ibility/Microp - one - etection | Correct | Log correctly reporte - Acce - ibility **not** gr - nte - (true - t - i -  - n - boxe - run never - it gr - nte - ) - n - printe - t - e ex - ct re - e - i - tion in - truction -  - re - l u - er woul -  - ee. |

T - i - confir - two t - ing - concretely r - t - er t - n by inference: (1) t - e rele - e buil -  - ctu - lly
run -  - it' - not ju - t `c - rgo c - eck`-cle - n, it opene -  - re - l overl - y win - ow on - re - l - i - pl - y - n -  - t - ye -  - live -  - n - (2) t - e " - oe - n't run Electron, - oul - be lig - ter" cl - i - fro - t - e co - petitive
brief -  - re - l nu - ber un - er it now (~119 MB i - le, no - o - el lo - e - ) in - te - of ju - t - n - rc - itectur - l - rgu - ent. It i - **not** yet - n - pple - -to- - pple - Wi - pr Flow co - p - ri - on - t - t - till
nee -  - o - eone to - ctu - lly in - t - ll Wi - pr Flow - n - run §4' - proce - ure - i - e-by- - i - e.

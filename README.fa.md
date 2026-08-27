<div dir="rtl" align="center">
  <img src="docs/assets/purify-mark.svg" width="88" alt="نشان Purify" />
  <h1>Purify | پیورفای</h1>
  <p><strong>دیسک ویندوز را تمیز کن؛ بدون تردید دربارهٔ اینکه چه چیزی امن است.</strong></p>
  <p>سریع، خصوصی و برگشت‌پذیر؛ ساخته‌شده با Rust و Tauri.</p>

  <p>
    <a href="https://github.com/Mahdi-mortazavi/purify/releases/latest"><strong>دانلود برای ویندوز</strong></a>
    · <a href="#how-it-works">نحوهٔ کار</a>
    · <a href="README.md">English</a>
    · <a href="https://github.com/Mahdi-mortazavi/purify/issues/new?template=bug_report.yml">گزارش باگ</a>
    · <a href="CONTRIBUTING.fa.md">مشارکت</a>
  </p>

  <p>
    <a href="https://github.com/Mahdi-mortazavi/purify/actions/workflows/ci.yml"><img src="https://github.com/Mahdi-mortazavi/purify/actions/workflows/ci.yml/badge.svg" alt="وضعیت CI" /></a>
    <a href="https://github.com/Mahdi-mortazavi/purify/releases/latest"><img src="https://img.shields.io/github/v/release/Mahdi-mortazavi/purify?sort=semver&color=0A84FF" alt="آخرین نسخه" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-34C759" alt="مجوز MIT" /></a>
  </p>
</div>

<br />

<div align="center">
  <img src="docs/assets/product-preview.svg" alt="پیش‌نمایش Purify؛ نقشهٔ دیسک و پیشنهادهای پاک‌سازی امن" width="920" />
</div>

## مسئله پیدا کردن فایل نیست؛ اعتماد به تصمیم است

وقتی فضای درایو ویندوز تمام می‌شود، یک ابزار فقط دیوار بلندی از پوشه‌ها نشان می‌دهد و ابزار دیگر از شما می‌خواهد کورکورانه حذف کنید. Purify مسیر آرام‌تری می‌سازد: **فضا را بفهم، دلیل هر پیشنهاد را ببین و بعد با امکان بازگشت پاک‌سازی کن.**

<a id="how-it-works"></a>
## چطور کار می‌کند؟

| ۱ · فهمیدن | ۲ · تصمیم‌گیری | ۳ · بازگشت |
| --- | --- | --- |
| درایو را اسکن کن و بزرگ‌ترین مصرف‌کننده‌ها را یک‌جا ببین. | پیشنهادهای قابل‌توضیح را با سطح اطمینان ببین: امن، احتمالاً امن یا نیازمند بررسی. | موارد تأییدشده ابتدا به قرنطینهٔ محلی می‌روند؛ هر زمان خواستی برگردانشان. حذف دائمی کاملاً صریح است. |

## چه چیزی دریافت می‌کنی؟

- **نقشهٔ دیسک** — نمایش سریع فضا با خواندن مستقیم NTFS/MFT در حالت مدیر سیستم و اسکن موازی قابل‌حمل در حالت عادی.
- **پاک‌سازی امن** — بیش از ۳۰ قاعده برای cache، فایل‌های باقی‌مانده و شلوغی‌های توسعه؛ هر مورد با دلیل ساده و قابل‌فهم.
- **قرنطینه** — پاک‌سازی به‌صورت پیش‌فرض برگشت‌پذیر است. هیچ حذف بی‌صدایی وجود ندارد.
- **مرتب‌سازی Downloads** — فایل‌های پراکنده را قبل از جابه‌جایی ببین و با یک Undo برگردان.
- **نگهبان دیسک** — پیش از بحرانی‌شدن فضا، فشار ذخیره‌سازی را ببین.
- **خصوصی از ابتدا** — آفلاین، بدون تله‌متری، بدون تغییر Registry و بدون وابستگی شبکه در هستهٔ محصول.

## چرا Purify؟

| | ابزارهای معمول پوشه | پاک‌کننده‌های یک‌کلیکی | **Purify** |
| --- | --- | --- | --- |
| فهمیدن محل مصرف فضا | ناقص | معمولاً نه | **نقشهٔ دیسک و بزرگ‌ترین مصرف‌کننده‌ها** |
| توضیح اینکه چه چیزی قابل حذف است | ندارد | گاهی | **دلیل و سطح اطمینان برای هر پیشنهاد** |
| امکان بازگشت | دستی | معمولاً ندارد | **قرنطینهٔ محلی و Restore** |
| کارکرد آفلاین | بله | متغیر | **بله، از ابتدا** |
| مناسب توسعه‌دهنده‌ها | نه | نه | **CLI، قوانین TOML و workspace در Rust** |

## شروع در ۶۰ ثانیه

۱. آخرین [نسخهٔ ویندوز](https://github.com/Mahdi-mortazavi/purify/releases/latest) را دانلود کن.
۲. Purify را باز کن و درایو یا پوشه را انتخاب کن.
۳. برای دیدن وضعیت فضا **Scan** و برای دریافت پیشنهادها **Analyze** را بزن.
۴. دلیل و سطح اطمینان را بخوان و با **Clean** موارد تأییدشده را به قرنطینه بفرست.

> به‌صورت پیش‌فرض هیچ فایلی حذف نمی‌شود؛ کنترل همیشه دست توست.

### حالت قابل‌اسکریپت

برای بررسی فقط‌خواندنی، اتوماسیون و عیب‌یابی CI:

```powershell
purify scan C:\ --top 20
purify analyze C:\Users\me
purify clean C:\Users\me                 # فقط پیش‌نمایش
purify clean C:\Users\me --apply         # قرنطینهٔ برگشت‌پذیر
purify restore <id>
```

## سریع، اما محتاط

با دسترسی مدیر، Purify مستقیماً Master File Table در NTFS را می‌خواند؛ بدون آن، از اسکن موازی استفاده می‌کند. الگوها یک‌بار کامپایل می‌شوند، مسیرها یک‌بار نرمال می‌شوند و اندازهٔ پوشه فقط وقتی لازم باشد محاسبه می‌شود.

در یک پروفایل نمونه با ۷٬۴۰۰ فایل، build نهایی حدود **۲۰ میلی‌ثانیه برای `scan`** و **۲۸ میلی‌ثانیه برای `analyze`** ثبت کرده است. نتیجه به دیسک و سخت‌افزار وابسته است؛ روش اندازه‌گیری در [`ARCHITECTURE.md`](ARCHITECTURE.md) آمده است.

## workspace کوچک، مرزهای روشن

| بخش | مسئولیت |
| --- | --- |
| `purify-core` | قوانین، پیشنهادها، قرنطینه، مرتب‌سازی و نگهبان؛ فقط Rust امن |
| `purify-ntfs` | تنها crate دارای دسترسی خام به volume/MFT |
| `purify-cli` | رابط خط فرمان قابل‌اسکریپت |
| `purify-desktop` | تجربهٔ دسکتاپ با Tauri 2 |
| `knowledge-base/` | قواعد پاک‌سازی قابل‌ویرایش توسط جامعه |

```powershell
git clone https://github.com/Mahdi-mortazavi/purify.git
cd purify
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI در هر Pull Request قالب‌بندی، Clippy، تست‌ها، build دسکتاپ روی Windows و Ubuntu و سیاست وابستگی‌ها را بررسی می‌کند.

## به لایهٔ اعتماد کمک کن

برای بهترکردن Purify لازم نیست متخصص Rust باشی. افزودن یک قاعدهٔ پاک‌سازی به `knowledge-base/` وقتی ارزشمند است که هدف، دلیل امن‌بودن و روش تستش روشن باشد.

۱. [`CONTRIBUTING.fa.md`](CONTRIBUTING.fa.md) و [`ARCHITECTURE.md`](ARCHITECTURE.md) را بخوان.
۲. از [Issueهای مناسب شروع](https://github.com/Mahdi-mortazavi/purify/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) یا یک بهبود کوچک و مشخص شروع کن.
۳. مرز ایمنی را صریح نگه دار و هر تغییر رفتاری را با تست قابل‌تکرار همراه کن.

گزارش باگ و بازخورد UX در [Issueها](https://github.com/Mahdi-mortazavi/purify/issues) خوش‌آمد است. نسخهٔ ویندوز، وضعیت دسترسی مدیر، صفحه یا فرمان درگیر و مسیر بازتولید امن را بنویس.

## نقشهٔ راه

- قواعد امن بیشتر برای ویندوز، همراه با توضیحات بهتر
- انتشار امضاشده با MSIX و Winget
- دسترسی‌پذیری و تجربهٔ کامل با صفحه‌کلید
- تاریخچهٔ سبک و قابل‌بررسی برای هر پاک‌سازی

## لینک‌های پروژه

- [نسخه‌ها](https://github.com/Mahdi-mortazavi/purify/releases) · [Issueها](https://github.com/Mahdi-mortazavi/purify/issues) · [گفت‌وگوها](https://github.com/Mahdi-mortazavi/purify/discussions)
- [معماری](ARCHITECTURE.md) · [مشارکت](CONTRIBUTING.fa.md) · [امنیت](SECURITY.md)
- English: [`README.md`](README.md)

## مجوز

MIT © Mahdi Mortazavi. متن کامل در [`LICENSE`](LICENSE) است.

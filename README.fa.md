# پیورفای | purify

<div dir="rtl" align="center">

**ببین چه چیزی دیسکت را پر کرده؛ بدون ترس آزادش کن.**

ابزار سریع، خصوصی و برگشت‌پذیر برای پاک‌سازی دیسک در ویندوز — ساخته‌شده با Rust و Tauri.

[![وضعیت CI](https://github.com/Mahdi-mortazavi/purify/actions/workflows/ci.yml/badge.svg)](https://github.com/Mahdi-mortazavi/purify/actions/workflows/ci.yml)
[![آخرین نسخه](https://img.shields.io/github/v/release/Mahdi-mortazavi/purify?sort=semver&color=0a84ff)](https://github.com/Mahdi-mortazavi/purify/releases/latest)
[![مجوز MIT](https://img.shields.io/badge/license-MIT-34c759.svg)](LICENSE)
[English](README.md) · [گزارش باگ](https://github.com/Mahdi-mortazavi/purify/issues/new) · [مشارکت](CONTRIBUTING.md)

</div>

وقتی فضای دیسک تمام می‌شود، ابزارهای معمول یا فقط انبوهی از پوشه‌ها را نشان می‌دهند یا از شما می‌خواهند فایل‌هایی را حذف کنید که نمی‌دانید چه هستند. **purify این فاصله را پر می‌کند:** اول بفهم چه چیزی بزرگ است، بعد تصمیم بگیر چه چیزی امن است، آن را به قرنطینهٔ برگشت‌پذیر منتقل کن و قبل از بحران بعدی باخبر شو.

به‌صورت پیش‌فرض هیچ فایلی حذف نمی‌شود. هر پیشنهاد سطح اطمینان و دلیل ساده و قابل‌فهم دارد.

## چه چیزهایی دریافت می‌کنی؟

- **نقشهٔ دیسک** — اسکن سریع با خواندن مستقیم NTFS/MFT یا حالت جایگزین قابل‌حمل، برای پیدا کردن بزرگ‌ترین مصرف‌کننده‌ها.
- **پاک‌سازی امن** — بیش از ۳۰ قانون برای cacheها، فایل‌های باقی‌مانده و شلوغی‌های توسعه، با سه سطح «امن»، «احتمالاً امن» و «نیازمند بررسی».
- **قرنطینه و بازگردانی** — پاک‌سازی فقط فایل‌ها را جابه‌جا می‌کند؛ هر زمان خواستی Restore کن. حذف دائمی جداگانه و با تأخیر انجام می‌شود.
- **مرتب‌سازی** — پیش‌نمایش و دسته‌بندی فایل‌های پراکندهٔ Downloads با امکان Undo.
- **نگهبان دیسک** — قبل از بحرانی شدن فشار فضای ذخیره‌سازی باخبر شو.
- **خصوصی و آفلاین** — بدون تله‌متری، بدون دستکاری Registry و بدون لمس فایل‌های محافظت‌شدهٔ سیستم.

## شروع در یک دقیقه

۱. آخرین [نسخهٔ ویندوز](https://github.com/Mahdi-mortazavi/purify/releases/latest) را دانلود کن.

۲. purify را باز کن و یک درایو انتخاب کن.

۳. برای دیدن وضعیت **Scan** و برای دریافت پیشنهادها **Analyze** را بزن.

۴. سطح اطمینان و دلیل هر مورد را بخوان؛ گزینهٔ **Clean** موارد تأییدشده را به قرنطینه منتقل می‌کند.

برای استفادهٔ اسکریپتی و فقط‌خواندنی:

```powershell
purify scan C:\ --top 20
purify analyze C:\Users\me
purify clean C:\Users\me                 # فقط پیش‌نمایش
purify clean C:\Users\me --apply         # قرنطینهٔ برگشت‌پذیر
purify restore <id>
```

## برای توسعه‌دهندگان

پروژه برای جدا نگه‌داشتن ریسک‌ها به چند بخش تقسیم شده است:

| بخش | مسئولیت |
| --- | --- |
| `purify-core` | قوانین، پیشنهادها، قرنطینه، مرتب‌سازی و نگهبان؛ فقط Rust امن |
| `purify-ntfs` | تنها بخشی که به volume و MFT خام دسترسی دارد |
| `purify-cli` | رابط خط فرمان و قابل‌اسکریپت |
| `purify-desktop` | رابط دسکتاپ با Tauri 2 |
| `knowledge-base/` | قوانین پاک‌سازی قابل ویرایش توسط جامعه |

```powershell
git clone https://github.com/Mahdi-mortazavi/purify.git
cd purify
cargo test --workspace --exclude purify-desktop
cargo fmt --all --check
cargo clippy --workspace --exclude purify-desktop --all-targets -- -D warnings
```

راهنمای کامل معماری در [`ARCHITECTURE.md`](ARCHITECTURE.md) و قوانین مشارکت در [`CONTRIBUTING.md`](CONTRIBUTING.md) قرار دارد. حتی افزودن یک قانون پاک‌سازی به Rust نیاز ندارد؛ فقط مرز ایمنی و تست آن را دقیق توضیح بده.

## مسیر پیش‌رو

- قوانین بیشتر برای سناریوهای امن ویندوز و توضیحات بهتر
- انتشار از طریق Winget/MSIX و installer امضاشده
- دسترس‌پذیری بهتر و تجربهٔ کامل‌تر با صفحه‌کلید

## مجوز

MIT © مهدی مرتضوی. متن کامل در [`LICENSE`](LICENSE) است.

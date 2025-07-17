# Windows Taskbar Manager

Bu uygulama Windows taskbar'ını tamamen gizler ve mouse event'lerini yakalayarak JSON formatında loglar.

## Özellikler

- ✅ Windows taskbar'ını tamamen gizleme
- ✅ Mouse cursor'ın taskbar alanına girme/çıkma event'lerini yakalama
- ✅ JSON formatında event logging
- ✅ Modern Rust ve Windows API kullanımı
- ✅ Async event handling
- ✅ Graceful shutdown (Ctrl+C)
- ✅ Güvenli cleanup ve emergency restore

## Gereksinimler

- Windows 10/11
- Rust 1.70+
- Yönetici yetkisi (administrator)

## Güvenlik Önlemleri

⚠️ **SÜPER GÜVENLİK SİSTEMİ**: Bu uygulama taskbar'ı gizlediği için çoklu güvenlik katmanları vardır:

### 🛡️ Güvenlik Katmanları:
1. **Normal çıkış**: Ctrl+C ile çıkışta taskbar otomatik restore edilir
2. **Panic handler**: Program crash olursa otomatik cleanup
3. **ATEXIT handler**: Process sonlanırken mutlaka çalışır  
4. **Signal handler**: Ctrl+C, SIGTERM gibi signalleri yakalar
5. **Drop implementation**: Scope sonunda otomatik cleanup
6. **Emergency restore**: `emergency_restore.bat` scripti
7. **Final fallback**: Explorer.exe restart

### 🚨 Zorla Kapatma Koruması:
- ✅ Task Manager "End Task" - KORUNUR
- ✅ `taskkill /f` komutu - KORUNUR  
- ✅ Process crash - KORUNUR
- ✅ System shutdown - KORUNUR
- ✅ Blue screen sonrası - Explorer restart ile düzelir

### 📋 Test Etme:
`test_force_close.bat` scriptini çalıştırarak güvenlik sistemini test edebilirsiniz.

## Kullanım

```bash
# Projeyi derle
cargo build --release

# Yönetici olarak çalıştır
run_as_admin.bat
```

Veya manuel olarak:
```bash
cargo run
```

## Acil Durum

Eğer taskbar kaybolursa:
1. `emergency_restore.bat` dosyasını çalıştırın
2. Veya PowerShell'de: `taskkill /f /im explorer.exe; start explorer.exe`

## Event Türleri

- `taskbar_hidden`: Taskbar gizlendiğinde
- `taskbar_shown`: Taskbar gösterildiğinde  
- `taskbar_restored`: Program kapanırken restore edildiğinde
- `mouse_request_show`: Mouse taskbar alanına girdiğinde
- `mouse_request_hide`: Mouse taskbar alanından çıktığında

## Örnek JSON Output

```json
{
  "event_type": "mouse_request_show",
  "timestamp": "2024-01-15T10:30:45.123Z",
  "mouse_position": {
    "x": 960,
    "y": 1070
  },
  "taskbar_state": "hidden_but_requested"
}
```

## Güvenlik

Bu uygulama sistem seviyesinde Windows API kullandığı için yönetici yetkisi gerektirir.

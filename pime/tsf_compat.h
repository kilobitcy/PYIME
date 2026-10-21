// mingw-w64 缺失的 TSF 声明(虚表顺序与 IID 取自 windows-rs 0.62 / win32metadata,即 Windows SDK)
#pragma once
#include <windows.h>
#include <msctf.h>
#include <ctfutb.h>

#define TF_CLIENTID_NULL 0

#define TF_CLUIE_DOCUMENTMGR 0x1
#define TF_CLUIE_COUNT 0x2
#define TF_CLUIE_SELECTION 0x4
#define TF_CLUIE_STRING 0x8
#define TF_CLUIE_PAGEINDEX 0x10
#define TF_CLUIE_CURRENTPAGE 0x20

#ifndef TF_LBI_ICON
#define TF_LBI_ICON 0x1
#define TF_LBI_TEXT 0x2
#define TF_LBI_TOOLTIP 0x4
#define TF_LBI_BITMAP 0x8
#define TF_LBI_BALLOON 0x10
#define TF_LBI_CUSTOMUI 0x20
#define TF_LBI_STATUS 0x10000
#endif
#define TF_LBI_BTNALL (TF_LBI_ICON | TF_LBI_TEXT | TF_LBI_TOOLTIP)
#ifndef TF_LBI_STYLE_BTN_BUTTON
#define TF_LBI_STYLE_BTN_BUTTON 0x10000
#define TF_LBI_STYLE_BTN_MENU 0x20000
#define TF_LBI_STYLE_BTN_TOGGLE 0x40000
#endif
#ifndef TF_LBI_STATUS_BTN_TOGGLED
#define TF_LBI_STATUS_BTN_TOGGLED 0x10000
#endif
#ifndef TF_LBMENUF_CHECKED
#define TF_LBMENUF_CHECKED 0x1
#define TF_LBMENUF_SUBMENU 0x2
#define TF_LBMENUF_SEPARATOR 0x4
#define TF_LBMENUF_RADIOCHECKED 0x8
#define TF_LBMENUF_GRAYED 0x10
#endif

typedef enum TfLBIClick { TF_LBI_CLK_RIGHT = 1, TF_LBI_CLK_LEFT = 2 } TfLBIClick;

MIDL_INTERFACE("6e4e2102-f9cd-433d-b496-303ce03a6507")
ITfTextInputProcessorEx : public ITfTextInputProcessor {
    virtual HRESULT STDMETHODCALLTYPE ActivateEx(ITfThreadMgr *ptim, TfClientId tid, DWORD dwFlags) = 0;
};
__CRT_UUID_DECL(ITfTextInputProcessorEx, 0x6e4e2102, 0xf9cd, 0x433d, 0xb4, 0x96, 0x30, 0x3c, 0xe0, 0x3a, 0x65, 0x07)

MIDL_INTERFACE("fee47777-163c-4769-996a-6e9c50ad8f54")
ITfDisplayAttributeProvider : public IUnknown {
    virtual HRESULT STDMETHODCALLTYPE EnumDisplayAttributeInfo(IEnumTfDisplayAttributeInfo **ppEnum) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetDisplayAttributeInfo(REFGUID guid, ITfDisplayAttributeInfo **ppInfo) = 0;
};
__CRT_UUID_DECL(ITfDisplayAttributeProvider, 0xfee47777, 0x163c, 0x4769, 0x99, 0x6a, 0x6e, 0x9c, 0x50, 0xad, 0x8f, 0x54)

MIDL_INTERFACE("6f8a98e4-aaa0-4f15-8c5b-07e0df0a3dd8")
ITfMenu : public IUnknown {
    virtual HRESULT STDMETHODCALLTYPE AddMenuItem(UINT uId, DWORD dwFlags, HBITMAP hbmp, HBITMAP hbmpMask,
        const WCHAR *pch, ULONG cch, ITfMenu **ppMenu) = 0;
};
__CRT_UUID_DECL(ITfMenu, 0x6f8a98e4, 0xaaa0, 0x4f15, 0x8c, 0x5b, 0x07, 0xe0, 0xdf, 0x0a, 0x3d, 0xd8)

MIDL_INTERFACE("28c7f1d0-de25-11d2-afdd-00105a2799b5")
ITfLangBarItemButton : public ITfLangBarItem {
    virtual HRESULT STDMETHODCALLTYPE OnClick(TfLBIClick click, POINT pt, const RECT *prcArea) = 0;
    virtual HRESULT STDMETHODCALLTYPE InitMenu(ITfMenu *pMenu) = 0;
    virtual HRESULT STDMETHODCALLTYPE OnMenuSelect(UINT wID) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetIcon(HICON *phIcon) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetText(BSTR *pbstrText) = 0;
};
__CRT_UUID_DECL(ITfLangBarItemButton, 0x28c7f1d0, 0xde25, 0x11d2, 0xaf, 0xdd, 0x00, 0x10, 0x5a, 0x27, 0x99, 0xb5)

MIDL_INTERFACE("db593490-098f-11d3-8df0-00105a2799b5")
ITfFunction : public IUnknown {
    virtual HRESULT STDMETHODCALLTYPE GetDisplayName(BSTR *pbstrName) = 0;
};
__CRT_UUID_DECL(ITfFunction, 0xdb593490, 0x098f, 0x11d3, 0x8d, 0xf0, 0x00, 0x10, 0x5a, 0x27, 0x99, 0xb5)

MIDL_INTERFACE("88f567c6-1757-49f8-a1b2-89234c1eeff9")
ITfFnConfigure : public ITfFunction {
    virtual HRESULT STDMETHODCALLTYPE Show(HWND hwndParent, LANGID langid, REFGUID rguidProfile) = 0;
};
__CRT_UUID_DECL(ITfFnConfigure, 0x88f567c6, 0x1757, 0x49f8, 0xa1, 0xb2, 0x89, 0x23, 0x4c, 0x1e, 0xef, 0xf9)

MIDL_INTERFACE("ea1ea138-19df-11d7-a6d2-00065b84435c")
ITfCandidateListUIElement : public ITfUIElement {
    virtual HRESULT STDMETHODCALLTYPE GetUpdatedFlags(DWORD *pdwFlags) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetDocumentMgr(ITfDocumentMgr **ppdim) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetCount(UINT *puCount) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetSelection(UINT *puIndex) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetString(UINT uIndex, BSTR *pstr) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetPageIndex(UINT *pIndex, UINT uSize, UINT *puPageCnt) = 0;
    virtual HRESULT STDMETHODCALLTYPE SetPageIndex(UINT *pIndex, UINT uPageCnt) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetCurrentPage(UINT *puPage) = 0;
};
__CRT_UUID_DECL(ITfCandidateListUIElement, 0xea1ea138, 0x19df, 0x11d7, 0xa6, 0xd2, 0x00, 0x06, 0x5b, 0x84, 0x43, 0x5c)

MIDL_INTERFACE("85fad185-58ce-497a-9460-355366b64b9a")
ITfCandidateListUIElementBehavior : public ITfCandidateListUIElement {
    virtual HRESULT STDMETHODCALLTYPE SetSelection(UINT nIndex) = 0;
    virtual HRESULT STDMETHODCALLTYPE Finalize() = 0;
    virtual HRESULT STDMETHODCALLTYPE Abort() = 0;
};
__CRT_UUID_DECL(ITfCandidateListUIElementBehavior, 0x85fad185, 0x58ce, 0x497a, 0x94, 0x60, 0x35, 0x53, 0x66, 0xb6, 0x4b, 0x9a)
#define UnadviseEventSink UnAdviseEventSink

#ifndef TF_LBI_STATUS_HIDDEN
#define TF_LBI_STATUS_HIDDEN 0x1
#endif
#ifndef TF_LBI_STATUS_DISABLED
#define TF_LBI_STATUS_DISABLED 0x2
#endif

#define IID_ITfTextInputProcessorEx __uuidof(ITfTextInputProcessorEx)
#define IID_ITfDisplayAttributeProvider __uuidof(ITfDisplayAttributeProvider)
#define IID_ITfMenu __uuidof(ITfMenu)
#define IID_ITfLangBarItemButton __uuidof(ITfLangBarItemButton)
#define IID_ITfFunction __uuidof(ITfFunction)
#define IID_ITfFnConfigure __uuidof(ITfFnConfigure)
#define IID_ITfCandidateListUIElement __uuidof(ITfCandidateListUIElement)
#define IID_ITfCandidateListUIElementBehavior __uuidof(ITfCandidateListUIElementBehavior)

static const GUID GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT = {0x13a016df, 0x560b, 0x46cd, {0x94, 0x7a, 0x4c, 0x3a, 0xf1, 0xe0, 0xe3, 0x5d}};
static const GUID GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT = {0xccf05dd7, 0x4a87, 0x11d7, {0xa6, 0xe2, 0x00, 0x06, 0x5b, 0x84, 0x43, 0x5c}};
static const GUID GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT = {0x25504fb4, 0x7bab, 0x4bc1, {0x9c, 0x69, 0xcf, 0x81, 0x89, 0x0f, 0x0e, 0xf5}};
static const GUID GUID_TFCAT_TIPCAP_UIELEMENTENABLED = {0x49d2f9cf, 0x1f5e, 0x11d7, {0xa6, 0xd3, 0x00, 0x06, 0x5b, 0x84, 0x43, 0x5c}};

//
//    Copyright (C) 2013 - 2020 Hong Jen Yee (PCMan) <pcman.tw@gmail.com>
//
//    This library is free software; you can redistribute it and/or
//    modify it under the terms of the GNU Library General Public
//    License as published by the Free Software Foundation; either
//    version 2 of the License, or (at your option) any later version.
//
//    This library is distributed in the hope that it will be useful,
//    but WITHOUT ANY WARRANTY; without even the implied warranty of
//    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
//    Library General Public License for more details.
//
//    You should have received a copy of the GNU Library General Public
//    License along with this library; if not, write to the
//    Free Software Foundation, Inc., 51 Franklin St, Fifth Floor,
//    Boston, MA  02110-1301, USA.
//

#include "CandidateWindow.h"
#include "DrawUtils.h"
#include "TextService.h"
#include "EditSession.h"

#include <algorithm>
#include <cassert>

#include <tchar.h>
#include <windows.h>

using namespace std;

namespace Ime {

CandidateWindow::CandidateWindow(TextService* service, EditSession* session):
    ImeWindow(service),
    shown_(false),
    candPerRow_(1),
    itemHeight_(0),
    currentSel_(0),
    hasResult_(false),
    useCursor_(true),
    selKeyFont_(NULL),
    selKeyOffsetY_(0),
    itemPadX_(0),
    itemPadY_(0),
    cornerRadius_(0),
    roundedByDwm_(false),
    selKeyWidth_(0) {

    if(service->isImmersive()) { // windows 8 app mode
        margin_ = 10;
        rowSpacing_ = 8;
        colSpacing_ = 12;
    }
    else { // desktop mode
        margin_ = 5;
        rowSpacing_ = 4;
        colSpacing_ = 8;
    }

    HWND parent = service->compositionWindow(session);
    create(parent, WS_POPUP|WS_CLIPCHILDREN, WS_EX_TOOLWINDOW|WS_EX_TOPMOST);

    // Win11:请 DWM 给窗口圆角(弹出窗口默认不圆角,需显式请求,见微软文档 Apply rounded corners
    // 示例 4)。动态取 DwmSetWindowAttribute,旧系统不支持该属性时返回失败,改为自绘 1px 边框。
    if(HMODULE dwm = ::LoadLibraryW(L"dwmapi.dll")) {
        typedef HRESULT (WINAPI *DwmSetWindowAttributeFn)(HWND, DWORD, LPCVOID, DWORD);
        auto setAttr = (DwmSetWindowAttributeFn)::GetProcAddress(dwm, "DwmSetWindowAttribute");
        const DWORD DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_ROUND = 2;
        DWORD pref = DWMWCP_ROUND;
        if(setAttr && hwnd_ && SUCCEEDED(setAttr(hwnd_, DWMWA_WINDOW_CORNER_PREFERENCE, &pref, sizeof(pref))))
            roundedByDwm_ = true;
        ::FreeLibrary(dwm);
    }
}

CandidateWindow::~CandidateWindow(void) {
    if(selKeyFont_)
        ::DeleteObject(selKeyFont_);
}

// ITfUIElement
STDMETHODIMP CandidateWindow::GetDescription(BSTR *pbstrDescription) {
    if (!pbstrDescription)
        return E_INVALIDARG;
    *pbstrDescription = SysAllocString(L"Candidate window~");
    return S_OK;
}

// {BD7CCC94-57CD-41D3-A789-AF47890CEB29}
STDMETHODIMP CandidateWindow::GetGUID(GUID *pguid) {
    if (!pguid)
        return E_INVALIDARG;
    *pguid = { 0xbd7ccc94, 0x57cd, 0x41d3, { 0xa7, 0x89, 0xaf, 0x47, 0x89, 0xc, 0xeb, 0x29 } };
    return S_OK;
}

STDMETHODIMP CandidateWindow::Show(BOOL bShow) {
    shown_ = bShow;
    if (shown_)
        show();
    else
        hide();
    return S_OK;
}

STDMETHODIMP CandidateWindow::IsShown(BOOL *pbShow) {
    if (!pbShow)
        return E_INVALIDARG;
    *pbShow = shown_;
    return S_OK;
}

// ITfCandidateListUIElement
STDMETHODIMP CandidateWindow::GetUpdatedFlags(DWORD *pdwFlags) {
    if (!pdwFlags)
        return E_INVALIDARG;
    /// XXX update all!!!
    *pdwFlags = TF_CLUIE_DOCUMENTMGR | TF_CLUIE_COUNT | TF_CLUIE_SELECTION | TF_CLUIE_STRING | TF_CLUIE_PAGEINDEX | TF_CLUIE_CURRENTPAGE;
    return S_OK;
}

STDMETHODIMP CandidateWindow::GetDocumentMgr(ITfDocumentMgr **ppdim) {
    if (!textService_)
        return E_FAIL;
    return textService_->currentContext()->GetDocumentMgr(ppdim);
}

STDMETHODIMP CandidateWindow::GetCount(UINT *puCount) {
    if (!puCount)
        return E_INVALIDARG;
    *puCount = std::min<UINT>(10, items_.size());
    return S_OK;
}

STDMETHODIMP CandidateWindow::GetSelection(UINT *puIndex) {
    assert(currentSel_ >= 0);
    if (!puIndex)
        return E_INVALIDARG;
    *puIndex = static_cast<UINT>(currentSel_);
    return S_OK;
}

STDMETHODIMP CandidateWindow::GetString(UINT uIndex, BSTR *pbstr) {
    if (!pbstr)
        return E_INVALIDARG;
    if (uIndex >= items_.size())
        return E_INVALIDARG;
    *pbstr = SysAllocString(items_[uIndex].c_str());
    return S_OK;
}

STDMETHODIMP CandidateWindow::GetPageIndex(UINT *puIndex, UINT uSize, UINT *puPageCnt) {
    /// XXX Always return the same single page index.
    if (!puPageCnt)
        return E_INVALIDARG;
    *puPageCnt = 1;
    if (puIndex) {
        if (uSize < *puPageCnt) {
            return E_INVALIDARG;
        }
        puIndex[0] = 0;
    }
    return S_OK;
}

STDMETHODIMP CandidateWindow::SetPageIndex(UINT *puIndex, UINT uPageCnt) {
    /// XXX Do not let app set page indices.
    if (!puIndex)
        return E_INVALIDARG;
    return S_OK;
}

STDMETHODIMP CandidateWindow::GetCurrentPage(UINT *puPage) {
    if (!puPage)
        return E_INVALIDARG;
    *puPage = 0;
    return S_OK;
}

LRESULT CandidateWindow::wndProc(UINT msg, WPARAM wp , LPARAM lp) {
    switch (msg) {
        case WM_PAINT:
            onPaint(wp, lp);
            break;
        case WM_ERASEBKGND:
            return TRUE;
            break;
        case WM_LBUTTONDOWN:
            onLButtonDown(wp, lp);
            break;
        case WM_MOUSEMOVE:
            onMouseMove(wp, lp);
            break;
        case WM_LBUTTONUP:
            onLButtonUp(wp, lp);
            break;
        case WM_MOUSEACTIVATE:
            return MA_NOACTIVATE;
        default:
            return Window::wndProc(msg, wp, lp);
    }
    return 0;
}

void CandidateWindow::onPaint(WPARAM wp, LPARAM lp) {
    // TODO: check isImmersive_, and draw the window differently
    // in Windows 8 app immersive mode to follow windows 8 UX guidelines
    PAINTSTRUCT ps;
    BeginPaint(hwnd_, &ps);
    HDC hDC = ps.hdc;
    HFONT oldFont;
    RECT rc;

    oldFont = (HFONT)SelectObject(hDC, font_);

    GetClientRect(hwnd_,&rc);
    ::SetBkMode(hDC, TRANSPARENT);

    // 白底;Win11 由 DWM 画圆角与细边框,否则自绘 1px 浅灰边框
    ::FillSolidRect(hDC, rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top, RGB(0xFF, 0xFF, 0xFF));
    if(!roundedByDwm_) {
        HBRUSH border = ::CreateSolidBrush(RGB(0xE5, 0xE5, 0xE5));
        ::FrameRect(hDC, &rc, border);
        ::DeleteObject(border);
    }

    // paint items
    for(int i = 0, n = items_.size(); i < n; ++i) {
        RECT rect;
        itemRect(i, rect);
        paintItem(hDC, i, rect);
    }
    SelectObject(hDC, oldFont);
    EndPaint(hwnd_, &ps);
}

void CandidateWindow::recalculateSize() {
    if(items_.empty()) {
        resize(margin_ * 2, margin_ * 2);
    }

    HDC hDC = ::GetWindowDC(hwnd());
    int height = 0;
    int width = 0;
    selKeyWidth_ = 0;
    textWidths_.assign(items_.size(), 0);
    itemHeight_ = 0;

    HGDIOBJ oldFont = ::SelectObject(hDC, font_);
    TEXTMETRIC tm;
    ::GetTextMetrics(hDC, &tm);
    // 尺寸参照微信输入法截图(em≈24px 时实测),按字号(em)换算以适配 DPI:
    // 每个候选是一个块(选中时为绿色圆角块),块内左右留白 0.42em(10-11px)、上下共 0.25em(块高 39px);
    // 块间距 0.67em(16px),块到窗口边 0.25em(6px),圆角半径 0.3em(7px);
    // 序号与候选间 1/4 字,序号字号为候选的 0.8 倍
    int em = tm.tmHeight - tm.tmInternalLeading;
    itemPadX_ = em * 5 / 12;
    itemPadY_ = em / 8;
    cornerRadius_ = em * 3 / 10;
    margin_ = em / 4;
    colSpacing_ = em * 2 / 3;
    rowSpacing_ = em / 4;
    LOGFONT lf;
    ::GetObject(font_, sizeof(lf), &lf);
    lf.lfHeight = lf.lfHeight * 4 / 5;
    if(selKeyFont_)
        ::DeleteObject(selKeyFont_);
    selKeyFont_ = ::CreateFontIndirect(&lf);
    ::SelectObject(hDC, selKeyFont_);
    TEXTMETRIC selKeyTm;
    ::GetTextMetrics(hDC, &selKeyTm);
    selKeyOffsetY_ = tm.tmAscent - selKeyTm.tmAscent; // 序号与候选基线对齐
    for(int i = 0, n = items_.size(); i < n; ++i) {
        SIZE selKeySize;
        wchar_t selKey = selKeys_[i];
        ::GetTextExtentPoint32W(hDC, &selKey, 1, &selKeySize);
        selKeyWidth_ = max(selKeyWidth_, int(selKeySize.cx) + em / 4);
    }
    ::SelectObject(hDC, font_);
    for(int i = 0, n = items_.size(); i < n; ++i) {

        // the candidate string
        SIZE candidateSize;
        wstring& item = items_.at(i);
        ::GetTextExtentPoint32W(hDC, item.c_str(), item.length(), &candidateSize);
        textWidths_[i] = candidateSize.cx;
        if(candidateSize.cy > itemHeight_)
            itemHeight_ = candidateSize.cy;
    }
    ::SelectObject(hDC, oldFont);
    ::ReleaseDC(hwnd(), hDC);

    // 窗口宽度取最宽一行的右边界
    width = height = margin_ * 2;
    for(int i = 0, n = items_.size(); i < n; ++i) {
        RECT rect;
        itemRect(i, rect);
        width = max(width, int(rect.right) + margin_);
        height = max(height, int(rect.bottom) + margin_);
    }
    resize(width, height);
}

void CandidateWindow::setCandPerRow(int n) {
    if(n != candPerRow_) {
        candPerRow_ = n;
        recalculateSize();
    }
}

bool CandidateWindow::filterKeyEvent(KeyEvent& keyEvent) {
    // select item with arrow keys
    int oldSel = currentSel_;
    switch(keyEvent.keyCode()) {
    case VK_UP:
        if(currentSel_ - candPerRow_ >=0)
            currentSel_ -= candPerRow_;
        break;
    case VK_DOWN:
        if(currentSel_ + candPerRow_ < items_.size())
            currentSel_ += candPerRow_;
        break;
    case VK_LEFT:
        if(currentSel_ - 1 >=0)
            --currentSel_;
        break;
    case VK_RIGHT:
        if(currentSel_ + 1 < items_.size())
            ++currentSel_;
        break;
    case VK_RETURN:
        hasResult_ = true;
        return true;
    default:
        return false;
    }
    // if currently selected item is changed, redraw
    if(currentSel_ != oldSel) {
        // repaint the old and new items
        RECT rect;
        itemRect(oldSel, rect);
        ::InvalidateRect(hwnd_, &rect, TRUE);
        itemRect(currentSel_, rect);
        ::InvalidateRect(hwnd_, &rect, TRUE);
        return true;
    }
    return false;
}

void CandidateWindow::setCurrentSel(int sel) {
    if(sel >= items_.size())
        sel = 0;
    if (currentSel_ != sel) {
        currentSel_ = sel;
        if (isVisible())
            ::InvalidateRect(hwnd_, NULL, TRUE);
    }
}

void CandidateWindow::clear() {
    items_.clear();
    selKeys_.clear();
    textWidths_.clear();
    currentSel_ = 0;
    hasResult_ = false;
}

void CandidateWindow::setUseCursor(bool use) {
    useCursor_ = use;
    if(isVisible())
        ::InvalidateRect(hwnd_, NULL, TRUE);
}

void CandidateWindow::paintItem(HDC hDC, int i, const RECT& itemRc) {
    bool selected = useCursor_ && i == currentSel_;
    if(selected) { // 选中:微信绿 #00B176 实心圆角块,序号与文字转白
        HBRUSH brush = ::CreateSolidBrush(RGB(0x00, 0xB1, 0x76));
        HPEN pen = ::CreatePen(PS_SOLID, 1, RGB(0x00, 0xB1, 0x76));
        HGDIOBJ oldBrush = ::SelectObject(hDC, brush);
        HGDIOBJ oldPen = ::SelectObject(hDC, pen);
        ::RoundRect(hDC, itemRc.left, itemRc.top, itemRc.right, itemRc.bottom, cornerRadius_ * 2, cornerRadius_ * 2);
        ::SelectObject(hDC, oldPen);
        ::SelectObject(hDC, oldBrush);
        ::DeleteObject(pen);
        ::DeleteObject(brush);
    }
    int x = itemRc.left + itemPadX_;
    int y = itemRc.top + itemPadY_;
    // 序号:小一号、灰色 #888(同微信输入法)
    wchar_t selKey = selKeys_[i];
    COLORREF oldColor = ::SetTextColor(hDC, selected ? RGB(0xFF, 0xFF, 0xFF) : RGB(0x88, 0x88, 0x88));
    HGDIOBJ oldFont = ::SelectObject(hDC, selKeyFont_);
    ::ExtTextOut(hDC, x, y + selKeyOffsetY_, 0, NULL, &selKey, 1, NULL);
    ::SelectObject(hDC, oldFont);
    // 候选文字 #333
    ::SetTextColor(hDC, selected ? RGB(0xFF, 0xFF, 0xFF) : RGB(0x33, 0x33, 0x33));
    wstring& item = items_.at(i);
    ::ExtTextOut(hDC, x + selKeyWidth_, y, 0, NULL, item.c_str(), item.length(), NULL);
    ::SetTextColor(hDC, oldColor); // restore text color
}

void CandidateWindow::itemRect(int i, RECT& rect) {
    int row, col;
    row = i / candPerRow_;
    col = i % candPerRow_;
    // 返回候选块(含块内留白)。同一行内,左边界 = 前面各块自身宽度 + 块间距之和
    int blockHeight = itemHeight_ + itemPadY_ * 2;
    rect.left = margin_;
    for(int j = i - col; j < i; ++j)
        rect.left += itemPadX_ * 2 + selKeyWidth_ + textWidths_[j] + colSpacing_;
    rect.top = margin_ + row * (blockHeight + rowSpacing_);
    rect.right = rect.left + itemPadX_ * 2 + selKeyWidth_ + textWidths_[i];
    rect.bottom = rect.top + blockHeight;
}


} // namespace Ime

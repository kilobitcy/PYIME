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
// PYIME 修改:候选按自身文字宽度排版(textWidths_)、增加序号字体/留白/圆角成员,paintItem 签名改传候选矩形。

#ifndef IME_CANDIDATE_WINDOW_H
#define IME_CANDIDATE_WINDOW_H

#include "ImeWindow.h"
#include <string>
#include <vector>
#include "ComObject.h"

namespace Ime {

class TextService;
class EditSession;
class KeyEvent;

// TODO: make the candidate window looks different in immersive mode
class CandidateWindow:
    public ImeWindow,
    public ComObject<ComInterface<ITfCandidateListUIElement>> {
public:
    CandidateWindow(TextService* service, EditSession* session);

    // ITfUIElement
    STDMETHODIMP GetDescription(BSTR *pbstrDescription);
    STDMETHODIMP GetGUID(GUID *pguid);
    STDMETHODIMP Show(BOOL bShow);
    STDMETHODIMP IsShown(BOOL *pbShow);

    // ITfCandidateListUIElement
    STDMETHODIMP GetUpdatedFlags(DWORD *pdwFlags);
    STDMETHODIMP GetDocumentMgr(ITfDocumentMgr **ppdim);
    STDMETHODIMP GetCount(UINT *puCount);
    STDMETHODIMP GetSelection(UINT *puIndex);
    STDMETHODIMP GetString(UINT uIndex, BSTR *pstr);
    STDMETHODIMP GetPageIndex(UINT *puIndex, UINT uSize, UINT *puPageCnt);
    STDMETHODIMP SetPageIndex(UINT *puIndex, UINT uPageCnt);
    STDMETHODIMP GetCurrentPage(UINT *puPage);

    const std::vector<std::wstring>& items() const {
        return items_;
    }

    void setItems(const std::vector<std::wstring>& items, const std::vector<wchar_t>& sekKeys) {
        items_ = items;
        selKeys_ = selKeys_;
        recalculateSize();
        refresh();
    }

    void add(std::wstring item, wchar_t selKey) {
        items_.push_back(item);
        selKeys_.push_back(selKey);
        // PYIME: 每个候选单独记录宽度
        textWidths_.push_back(0); // 与 items_ 保持等长,宽度在 recalculateSize() 中计算
    }

    void clear();

    int candPerRow() const {
        return candPerRow_;
    }
    void setCandPerRow(int n);

    virtual void recalculateSize();

    bool filterKeyEvent(KeyEvent& keyEvent);

    int currentSel() const {
        return currentSel_;
    }
    void setCurrentSel(int sel);

    wchar_t currentSelKey() const {
        return selKeys_.at(currentSel_);
    }

    bool hasResult() const {
        return hasResult_;
    }

    bool useCursor() const {
        return useCursor_;
    }

    void setUseCursor(bool use);

protected:
    LRESULT wndProc(UINT msg, WPARAM wp , LPARAM lp);
    void onPaint(WPARAM wp, LPARAM lp);
    // PYIME: 参数由 (x, y) 改为候选矩形
    void paintItem(HDC hDC, int i, const RECT& itemRc);
    void itemRect(int i, RECT& rect);

protected: // COM object should not be deleted directly. calling Release() instead.
    ~CandidateWindow(void);

private:
    BOOL shown_;

    int selKeyWidth_;
    // PYIME: 取代统一的 textWidth_
    std::vector<int> textWidths_; // 每个候选按自身文字宽度排版,不再统一取最宽
    int itemHeight_;
    int candPerRow_;
    int colSpacing_;
    int rowSpacing_;
    std::vector<wchar_t> selKeys_;
    std::vector<std::wstring> items_;
    int currentSel_;
    bool hasResult_;
    bool useCursor_;
    // PYIME: 以下成员为新增,用于微信风格的候选外观
    HFONT selKeyFont_; // 序号用的小号字体
    int selKeyOffsetY_; // 序号相对候选的下移量,使两者基线对齐
    int itemPadX_; // 候选块内左右留白
    int itemPadY_; // 候选块内上下留白
    int cornerRadius_; // 选中块圆角半径
    bool roundedByDwm_; // Win11 DWM 已给窗口圆角(并画边框)
};

}

#endif

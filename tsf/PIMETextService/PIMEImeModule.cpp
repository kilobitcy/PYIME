//
//	Copyright (C) 2013 Hong Jen Yee (PCMan) <pcman.tw@gmail.com>
//
//	This library is free software; you can redistribute it and/or
//	modify it under the terms of the GNU Library General Public
//	License as published by the Free Software Foundation; either
//	version 2 of the License, or (at your option) any later version.
//
//	This library is distributed in the hope that it will be useful,
//	but WITHOUT ANY WARRANTY; without even the implied warranty of
//	MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
//	Library General Public License for more details.
//
//	You should have received a copy of the GNU Library General Public
//	License along with this library; if not, write to the
//	Free Software Foundation, Inc., 51 Franklin St, Fifth Floor,
//	Boston, MA  02110-1301, USA.
//
// PYIME 修改:去掉 backends.json/ime.json 扫描与设置工具,换新 CLSID。

#include "PIMEImeModule.h"
#include "PIMETextService.h"

namespace PIME {

// CLSID of our Text service
// PYIME:新 CLSID,与官方 PIME 的 {35F67E9D-A54D-4177-9697-8B0AB71A9E04} 不同,两者可并存
// {8298C4A9-D032-442A-90AE-1983DF71CDBE}
const GUID g_textServiceClsid =
{ 0x8298c4a9, 0xd032, 0x442a, { 0x90, 0xae, 0x19, 0x83, 0xdf, 0x71, 0xcd, 0xbe } };

ImeModule::ImeModule(HMODULE module):
	Ime::ImeModule(module, g_textServiceClsid) {
}

ImeModule::~ImeModule(void) {
}

// virtual
Ime::TextService* ImeModule::createTextService() {
	TextService* service = new PIME::TextService(this);
	return service;
}

} // namespace PIME

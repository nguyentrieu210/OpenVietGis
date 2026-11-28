import {ApiError} from "@openvietgis/client-sdk";
export const msg=(e:unknown)=>e instanceof ApiError?e.status===401?"Phiên đăng nhập đã hết hạn.":e.status===403?"Bạn không có quyền thực hiện thao tác này.":e.status===409?"Dữ liệu đã thay đổi hoặc có xung đột. Hãy kiểm tra lại.":"API trả về HTTP "+e.status:e instanceof Error?e.message:"Không thể thực hiện thao tác.";

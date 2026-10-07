// Sinh tự động từ vi-du/diem-so.an — sửa tệp nguồn, đừng sửa tệp này.
import java.util.*;
// Tên đã đổi sang ASCII:
//   Điểm Số -> Diem_So
//   xếp loại -> xep_loai
//   điểm trung bình -> diem_trung_binh
//   các điểm -> cac_diem
//   tổng điểm -> tong_diem
//   số môn -> so_mon
//   điểm -> diem
//   kết quả -> ket_qua

public class Diem_So { // dòng 2
    static String xep_loai(double diem_trung_binh) { // dòng 3
        if (diem_trung_binh >= 8) { // dòng 4
            return "giỏi"; // dòng 5
        }
        if (diem_trung_binh >= 6.5) { // dòng 7
            return "khá"; // dòng 8
        }
        return "trung bình"; // dòng 10
    }
    public static void main(String[] args) { // dòng 13
        var cac_diem = new ArrayList<>(List.of(9, 7.5, -1, 8, 100, 6)); // dòng 14
        double tong_diem = 0; // dòng 15
        var so_mon = 0; // dòng 16
        for (var diem : cac_diem) { // dòng 18
            if (diem < 0) { // dòng 19
                continue; // dòng 20
            }
            if (diem > 10) { // dòng 22
                System.out.println("Gặp điểm " + diem + " không hợp lệ, dừng lại."); // dòng 23
                break; // dòng 24
            }
            tong_diem += diem; // dòng 26
            so_mon += 1; // dòng 27
        }
        var diem_trung_binh = tong_diem / so_mon; // dòng 30
        var ket_qua = xep_loai(diem_trung_binh); // dòng 31
        System.out.println("Trung bình " + so_mon + " môn: " + diem_trung_binh + ", xếp loại " + ket_qua); // dòng 32
        if (Objects.equals(ket_qua, "giỏi")) { // dòng 34
            System.out.println("Chúc mừng! Tên xếp loại dài " + (ket_qua.length()) + " chữ."); // dòng 35
        }
    }
}

class Anature { static final Scanner NHAP = new Scanner(System.in); }

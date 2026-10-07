// Sinh tự động từ vi-du/danh-sach.an — sửa tệp nguồn, đừng sửa tệp này.
import java.util.*;
// Tên đã đổi sang ASCII:
//   DanhSách -> DanhSach
//   tổng -> tong
//   kết_quả -> ket_qua
//   tên -> ten
//   bình_phương -> binh_phuong
//   điểm -> diem

public class DanhSach { // dòng 2
    static int tong(ArrayList<Integer> ds) { // dòng 3
        var ket_qua = 0; // dòng 4
        for (var x : ds) { // dòng 5
            ket_qua = ket_qua + x; // dòng 6
        }
        return ket_qua; // dòng 8
    }
    public static void main(String[] args) { // dòng 11
        System.out.println("Bạn tên gì?"); // dòng 12
        var ten = Anature.NHAP.nextLine(); // dòng 13
        System.out.println("Đếm đến mấy?"); // dòng 14
        var n = Integer.parseInt(Anature.NHAP.nextLine().trim()); // dòng 15
        ArrayList<Integer> binh_phuong = new ArrayList<>(List.of()); // dòng 17
        for (var i = 1; i <= n; i++) { // dòng 18
            binh_phuong.add(i * i); // dòng 19
        }
        System.out.println("Chào " + ten + ", bình phương: " + binh_phuong); // dòng 21
        binh_phuong.set(0, 100); // dòng 23
        binh_phuong.remove((int) (1)); // dòng 24
        System.out.println("Sau khi sửa: " + binh_phuong); // dòng 25
        System.out.println("Phần tử cuối: " + binh_phuong.get((binh_phuong.size()) - 1)); // dòng 26
        System.out.println("Tổng: " + tong(binh_phuong)); // dòng 27
        var diem = new ArrayList<>(List.of(7, 8, 9)); // dòng 29
        System.out.println("Điểm đầu: " + diem.get(0) + ", số điểm: " + (diem.size())); // dòng 30
    }
}

class Anature { static final Scanner NHAP = new Scanner(System.in); }

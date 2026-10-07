// Sinh tự động từ vi-du/ky-hieu.an — sửa tệp nguồn, đừng sửa tệp này.
import java.util.*;
// Tên đã đổi sang ASCII:
//   KýHiệu -> KyHieu
//   tổng -> tong
//   kết_quả -> ket_qua
//   bình_phương -> binh_phuong

public class KyHieu { // dòng 2
    static int tong(ArrayList<Integer> ds) { // dòng 3
        var ket_qua = 0; // dòng 4
        for (var x : ds) { // dòng 5
            ket_qua += x; // dòng 6
        }
        return ket_qua; // dòng 8
    }
    public static void main(String[] args) { // dòng 11
        ArrayList<Integer> binh_phuong = new ArrayList<>(List.of()); // dòng 12
        for (var i = 1; i <= 5; i++) { // dòng 13
            binh_phuong.add(i * i); // dòng 14
        }
        System.out.println("bình phương: " + binh_phuong); // dòng 16
        var t = tong(binh_phuong); // dòng 18
        if ((t >= 50) && ((binh_phuong.size()) != 0)) { // dòng 19
            System.out.println("tổng = " + t + ", trung bình = " + (t / (binh_phuong.size()))); // dòng 20
        }
    }
}

class Anature { static final Scanner NHAP = new Scanner(System.in); }

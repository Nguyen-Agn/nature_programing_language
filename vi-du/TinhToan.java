// Sinh tự động từ vi-du/ham.an — sửa tệp nguồn, đừng sửa tệp này.
// Tên đã đổi sang ASCII:
//   TínhToán -> TinhToan
//   tổng -> tong
//   giai_thừa -> giai_thua
//   chào -> chao
//   tên -> ten
//   Tông -> Tong_2
//   tỉ_lệ -> ti_le
//   lớn -> lon

public class TinhToan { // dòng 2
    static int tong(int a, int b) { // dòng 3
        return a + b; // dòng 4
    }
    static long giai_thua(long n) { // dòng 7
        if (n <= 1) { // dòng 8
            return 1; // dòng 9
        }
        return n * giai_thua(n - 1); // dòng 11
    }
    static void chao(String ten) { // dòng 14
        System.out.println("Xin chào, " + ten + "!"); // dòng 15
    }
    public static void main(String[] args) { // dòng 18
        chao("Thái"); // dòng 19
        var tong = tong(2, 3); // dòng 20
        var Tong_2 = tong(10, 20); // dòng 21
        System.out.println("Tổng = " + tong + ", Tông = " + Tong_2); // dòng 22
        System.out.println("10! = " + giai_thua(10)); // dòng 23
        double ti_le = 7 / 2.0; // dòng 25
        boolean lon = (ti_le > 3) && (Tong_2 > tong); // dòng 26
        System.out.println("tỉ lệ = " + ti_le + ", lớn = " + lon); // dòng 27
    }
}

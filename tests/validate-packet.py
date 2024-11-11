import re
import socket

# Validation rule: Messages should match "Test message X" where X is an integer
def is_valid_message(message):
    return bool(re.match(r"^Test message \d+$", message))

def validate_messages():
    server_address = ('127.0.0.1', 8089)  # Match with TCPServer's IP and port
    buffer_size = 1024

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server_socket:
        server_socket.bind(server_address)
        server_socket.listen(1)
        print("Validation Server started. Waiting for connections...")

        while True:
            conn, addr = server_socket.accept()
            with conn:
                print(f"Connected by {addr}")
                while True:
                    data = conn.recv(buffer_size)
                    if not data:
                        break
                    message = data.decode()
                    print(f"Received message: {message}")
                    
                    if is_valid_message(message):
                        print("Message is valid.")
                    else:
                        print("Message is invalid.")

if __name__ == "__main__":
    validate_messages()
